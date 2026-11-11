//! Admin password file, token verification, and session records.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, password_hash::rand_core::OsRng};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::db::{DataPaths, unix_now_ms};

/// Session cookie name (`HttpOnly`, `Path=/`, `SameSite=Lax`, no `Secure`).
pub const SESSION_COOKIE: &str = "x_loadgen_session";

/// Minimum length of a changed password token.
pub const MIN_NEW_TOKEN_LEN: usize = 16;

/// Session lifetime used when issuing and refreshing sessions.
pub const SESSION_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Log line for the password file (path only, never the token).
#[must_use]
pub fn password_file_log_line(path: &Path) -> String {
    format!("admin password file: {}", path.display())
}

/// Generate a 32-byte CSPRNG token encoded as 64 lowercase hex characters.
///
/// # Errors
///
/// Returns an error if the operating system random generator fails.
pub fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).context("read CSPRNG for token")?;
    Ok(hex_encode(&bytes))
}

/// Ensure `<data-dir>/admin.passwd` exists (mode 600). Existing files are reused.
///
/// # Errors
///
/// Returns an error if the data directory or file cannot be created, written,
/// or given mode 600, or if an existing file is empty. The path is included
/// in the error message.
pub fn ensure_admin_passwd(paths: &DataPaths) -> Result<String> {
    paths
        .ensure()
        .with_context(|| format!("create data directory {}", paths.root().display()))?;
    let path = paths.admin_passwd();
    if path.is_file() {
        return read_admin_passwd(&path);
    }
    let token = random_token()?;
    write_admin_passwd(&path, &token, false)?;
    Ok(token)
}

/// Overwrite `admin.passwd`, clear the stored hash, require a new change, and
/// drop every session.
///
/// # Errors
///
/// Returns an error if the file or the central database cannot be updated.
pub fn reset_passwd(paths: &DataPaths) -> Result<()> {
    paths
        .ensure()
        .with_context(|| format!("create data directory {}", paths.root().display()))?;
    let token = random_token()?;
    write_admin_passwd(&paths.admin_passwd(), &token, true)?;
    let conn = crate::db::open_central(paths)?;
    replace_auth_state(&conn, None, true)?;
    delete_all_sessions(&conn)?;
    Ok(())
}

/// Read and trim an existing password file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or is empty after trim.
pub fn read_admin_passwd(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read admin password file {}", path.display()))?;
    let token = raw.trim().to_string();
    ensure!(
        !token.is_empty(),
        "admin password file {} is empty",
        path.display()
    );
    Ok(token)
}

/// Write `token` (plus a trailing newline) and set mode 600.
///
/// # Errors
///
/// Returns an error if the file cannot be written or permissions cannot be set.
pub fn write_admin_passwd(path: &Path, token: &str, overwrite: bool) -> Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true);
    if overwrite {
        opts.truncate(true);
    } else {
        opts.create_new(true);
    }
    let mut file = opts
        .open(path)
        .with_context(|| format!("open admin password file {}", path.display()))?;
    file.write_all(token.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .with_context(|| format!("write admin password file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync admin password file {}", path.display()))?;
    set_mode_600(path)?;
    Ok(())
}

/// Constant-time equality for two token strings. Different lengths compare as unequal.
#[must_use]
pub fn tokens_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        let _ = left.as_bytes().ct_eq(left.as_bytes());
        return false;
    }
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

/// Argon2id password hash for a token.
///
/// # Errors
///
/// Returns an error if hashing fails.
pub fn hash_password(token: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| anyhow::anyhow!("hash admin token with argon2id: {err}"))
}

/// Verify a token against an argon2id hash.
///
/// # Errors
///
/// Returns an error if the stored hash is malformed.
pub fn verify_password(token: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|err| anyhow::anyhow!("parse stored password hash: {err}"))?;
    match Argon2::default().verify_password(token.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(err) => Err(anyhow::anyhow!("verify password hash: {err}")),
    }
}

/// SHA-256 hex digest of a session token (stored in `sessions.token_hash`).
#[must_use]
pub fn session_token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex_encode(&digest)
}

/// Current admin authentication row (`id = 1`), if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthState {
    pub password_hash: Option<String>,
    pub must_change: bool,
    pub updated_at: i64,
}

/// Load `auth_state`, or a default "must change" row when the table is empty.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn load_auth_state(conn: &Connection) -> Result<AuthState> {
    let row = conn
        .query_row(
            "SELECT password_hash, must_change, updated_at FROM auth_state WHERE id = 1",
            [],
            |row| {
                Ok(AuthState {
                    password_hash: row.get(0)?,
                    must_change: row.get::<_, i64>(1)? != 0,
                    updated_at: row.get(2)?,
                })
            },
        )
        .optional()
        .context("load auth_state")?;
    Ok(row.unwrap_or(AuthState {
        password_hash: None,
        must_change: true,
        updated_at: 0,
    }))
}

/// Insert or replace the single `auth_state` row.
///
/// # Errors
///
/// Returns an error if the row cannot be written.
pub fn replace_auth_state(
    conn: &Connection,
    password_hash: Option<&str>,
    must_change: bool,
) -> Result<()> {
    let now = i64::try_from(unix_now_ms()?).unwrap_or(i64::MAX);
    conn.execute(
        "INSERT INTO auth_state (id, password_hash, must_change, updated_at)
         VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
            password_hash = excluded.password_hash,
            must_change = excluded.must_change,
            updated_at = excluded.updated_at",
        params![password_hash, i64::from(must_change), now],
    )
    .context("write auth_state")?;
    Ok(())
}

/// Current credential used for login and password change.
pub enum Credential {
    /// Plain token still stored in `admin.passwd`.
    Bootstrap(String),
    /// Argon2id hash already stored in SQLite.
    Hash(String),
}

/// Resolve the credential used to log in. Re-reads the password file when no
/// hash has been stored yet.
///
/// # Errors
///
/// Returns an error if the password file cannot be read while a change is
/// still required.
pub fn current_credential(paths: &DataPaths, state: &AuthState) -> Result<Credential> {
    if let Some(hash) = state.password_hash.as_deref() {
        return Ok(Credential::Hash(hash.to_string()));
    }
    let token = read_admin_passwd(&paths.admin_passwd())?;
    Ok(Credential::Bootstrap(token))
}

/// Check a presented login/old token against the current credential.
///
/// # Errors
///
/// Returns an error if hash verification fails internally.
pub fn verify_credential(presented: &str, credential: &Credential) -> Result<bool> {
    match credential {
        Credential::Bootstrap(expected) => Ok(tokens_equal(presented, expected)),
        Credential::Hash(hash) => verify_password(presented, hash),
    }
}

/// Create a new session and return the raw session token (not the hash).
///
/// # Errors
///
/// Returns an error if a row cannot be inserted.
pub fn issue_session(conn: &Connection) -> Result<String> {
    let token = random_token()?;
    let hash = session_token_hash(&token);
    let now = i64::try_from(unix_now_ms()?).unwrap_or(i64::MAX);
    let expires = now.saturating_add(SESSION_TTL_MS);
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sessions (id, token_hash, created_at, last_seen_at, expires_at)
         VALUES (?1, ?2, ?3, ?3, ?4)",
        params![id, hash, now, expires],
    )
    .context("insert session")?;
    Ok(token)
}

/// A session that has been looked up and is still valid.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub expires_at: i64,
}

/// Look up a session by the raw token, refresh `last_seen_at` when valid.
///
/// # Errors
///
/// Returns an error if the query or update fails. Expired or unknown tokens
/// yield `Ok(None)`.
pub fn touch_session(conn: &Connection, token: &str) -> Result<Option<Session>> {
    let hash = session_token_hash(token);
    let now = i64::try_from(unix_now_ms()?).unwrap_or(i64::MAX);
    let session = conn
        .query_row(
            "SELECT id, expires_at FROM sessions WHERE token_hash = ?1",
            params![hash],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    expires_at: row.get(1)?,
                })
            },
        )
        .optional()
        .context("lookup session")?;
    let Some(session) = session else {
        return Ok(None);
    };
    if session.expires_at <= now {
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![session.id])
            .context("delete expired session")?;
        return Ok(None);
    }
    let expires = now.saturating_add(SESSION_TTL_MS);
    conn.execute(
        "UPDATE sessions SET last_seen_at = ?1, expires_at = ?2 WHERE id = ?3",
        params![now, expires, session.id],
    )
    .context("refresh session")?;
    Ok(Some(session))
}

/// Delete one session by the raw token (logout).
///
/// # Errors
///
/// Returns an error if the statement fails.
pub fn delete_session(conn: &Connection, token: &str) -> Result<()> {
    let hash = session_token_hash(token);
    conn.execute("DELETE FROM sessions WHERE token_hash = ?1", params![hash])
        .context("delete session")?;
    Ok(())
}

/// Delete every session except `keep_id` (if any).
///
/// # Errors
///
/// Returns an error if the statement fails.
pub fn delete_other_sessions(conn: &Connection, keep_id: Option<&str>) -> Result<()> {
    if let Some(id) = keep_id {
        conn.execute("DELETE FROM sessions WHERE id != ?1", params![id])
            .context("delete other sessions")?;
    } else {
        delete_all_sessions(conn)?;
    }
    Ok(())
}

/// Delete every session.
///
/// # Errors
///
/// Returns an error if the statement fails.
pub fn delete_all_sessions(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM sessions", [])
        .context("delete all sessions")?;
    Ok(())
}

/// Recoverable failures when changing the admin token.
#[derive(Debug, thiserror::Error)]
pub enum ChangePasswordError {
    /// Presented `old_token` did not match the current credential.
    #[error("old token is incorrect")]
    IncorrectOld,
    /// `new_token` is shorter than [`MIN_NEW_TOKEN_LEN`].
    #[error("new token must be at least {min} characters")]
    TooShort {
        /// Required minimum length.
        min: usize,
    },
    /// `new_token` equals `old_token`.
    #[error("new token must differ from the old token")]
    SameAsOld,
    /// Storage or hashing failure.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Change the admin token: write the hash, delete `admin.passwd`, drop other sessions.
///
/// If the password file cannot be deleted, the database is left unchanged.
///
/// # Errors
///
/// Returns an error on validation or I/O failure. The caller maps these to HTTP.
pub fn change_password(
    paths: &DataPaths,
    conn: &mut Connection,
    old_token: &str,
    new_token: &str,
    keep_session_id: Option<&str>,
) -> Result<(), ChangePasswordError> {
    let state = load_auth_state(conn)?;
    let credential = current_credential(paths, &state)?;
    if !verify_credential(old_token, &credential)? {
        return Err(ChangePasswordError::IncorrectOld);
    }
    if new_token.len() < MIN_NEW_TOKEN_LEN {
        return Err(ChangePasswordError::TooShort {
            min: MIN_NEW_TOKEN_LEN,
        });
    }
    if tokens_equal(old_token, new_token) {
        return Err(ChangePasswordError::SameAsOld);
    }
    let hash = hash_password(new_token)?;
    let tx = conn.transaction().context("begin password change")?;
    replace_auth_state(&tx, Some(&hash), false)?;
    let file = paths.admin_passwd();
    if file.exists() {
        fs::remove_file(&file).map_err(|err| {
            anyhow::anyhow!(
                "delete admin password file {} after hashing the new token: {err}",
                file.display()
            )
        })?;
    }
    tx.commit().context("commit password change")?;
    delete_other_sessions(conn, keep_session_id)?;
    Ok(())
}

/// Format a `Set-Cookie` value for a new session (no `Secure`).
#[must_use]
pub fn session_cookie(token: &str) -> String {
    let max_age = SESSION_TTL_MS / 1000;
    format!("{SESSION_COOKIE}={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age={max_age}")
}

/// Format a `Set-Cookie` value that clears the session cookie.
#[must_use]
pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0")
}

fn set_mode_600(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod 600 {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for &byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_admin_passwd, hash_password, password_file_log_line, random_token, tokens_equal,
        verify_password,
    };
    use crate::db::DataPaths;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "x-loadgen-auth-{}-{}-{n}-{tag}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis())
            ));
            fs::create_dir_all(&path).expect("temp dir");
            Self { path }
        }

        fn paths(&self) -> DataPaths {
            DataPaths::new(&self.path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn random_token_is_64_lowercase_hex() {
        let token = random_token().expect("rng");
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
        assert_ne!(token, random_token().expect("rng"));
    }

    #[test]
    fn ensure_reuses_existing_password_file_and_sets_mode_600() {
        let tmp = TempDir::new("file");
        let paths = tmp.paths();
        let first = ensure_admin_passwd(&paths).expect("first");
        let again = ensure_admin_passwd(&paths).expect("second");
        assert_eq!(first, again);
        let mode = fs::metadata(paths.admin_passwd())
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let logged = password_file_log_line(&paths.admin_passwd());
        assert!(logged.contains(&paths.admin_passwd().display().to_string()));
        assert!(!logged.contains(&first), "log must not include the token");
    }

    #[test]
    fn tokens_equal_is_length_safe() {
        assert!(tokens_equal("abcd", "abcd"));
        assert!(!tokens_equal("abcd", "abce"));
        assert!(!tokens_equal("short", "longer"));
    }

    #[test]
    fn argon2id_roundtrip() {
        let hash = hash_password("sixteen-chars-ok").expect("hash");
        assert!(hash.contains("argon2id"));
        assert!(verify_password("sixteen-chars-ok", &hash).expect("ok"));
        assert!(!verify_password("sixteen-chars-no", &hash).expect("no"));
    }
}
