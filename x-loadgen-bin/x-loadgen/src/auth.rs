//! Admin password file, login-token verification, and an in-memory token cache.
//!
//! The login token is the only credential. It never expires on its own and
//! changes only when the password is changed or `reset-passwd` is run. The
//! token is stored on disk as an argon2id hash (or, before the first change,
//! as plaintext in `admin.passwd`). After the first successful argon2
//! verification, subsequent requests compare the presented token with the
//! cached plaintext in constant time.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, ensure};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, password_hash::rand_core::OsRng};
use rusqlite::{Connection, OptionalExtension, params};
use subtle::ConstantTimeEq;

use crate::db::{DataPaths, unix_now_ms};

/// Auth cookie name (`HttpOnly`, `Path=/`, `SameSite=Lax`, no `Secure`, no
/// `Max-Age`). The cookie holds the login token itself.
pub const AUTH_COOKIE: &str = "x_loadgen_token";

/// Minimum length of a changed password token.
pub const MIN_NEW_TOKEN_LEN: usize = 24;

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

/// Overwrite `admin.passwd`, clear the stored hash, and require a new change.
/// A running server picks this up on the next request via `auth_state.updated_at`.
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

/// Current admin authentication row (`id = 1`), if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthState {
    pub password_hash: Option<String>,
    pub must_change: bool,
    pub updated_at: i64,
}

impl AuthState {
    /// `true` when the bootstrap token is still in force and must be changed.
    #[must_use]
    pub fn must_change_password(&self) -> bool {
        self.password_hash.is_none() || self.must_change
    }
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
/// Bootstrap tokens are compared in constant time against the file. Hashed
/// tokens run argon2id. Prefer [`TokenCache::verify`] on the request path so
/// argon2 runs only once until the password changes.
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

/// In-memory cache of a successfully verified login token.
///
/// The password is never stored on disk in a weaker form. After the first
/// successful argon2 verification, later checks compare against this cached
/// value in constant time. The cache is keyed by the stored hash and
/// `auth_state.updated_at`, so a `reset-passwd` from another process is
/// noticed on the next request. [`TokenCache::invalidate`] is used after an
/// in-process password change.
#[derive(Clone, Default)]
pub struct TokenCache {
    inner: Arc<Mutex<Option<CachedToken>>>,
}

struct CachedToken {
    token: String,
    password_hash: String,
    updated_at: i64,
}

impl TokenCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop any cached token. Called after an in-process password change.
    pub fn invalidate(&self) {
        *self.lock() = None;
    }

    /// Verify `presented` against `credential`.
    ///
    /// Bootstrap credentials always compare against the file (cheap, no
    /// argon2). Hash credentials use the cache when `updated_at` and the hash
    /// still match; otherwise argon2 runs and a successful result is stored.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored hash is malformed.
    pub fn verify(
        &self,
        presented: &str,
        credential: &Credential,
        updated_at: i64,
    ) -> Result<bool> {
        match credential {
            Credential::Bootstrap(expected) => Ok(tokens_equal(presented, expected)),
            Credential::Hash(hash) => {
                if let Some(cached) = self.lock().as_ref()
                    && cached.updated_at == updated_at
                    && cached.password_hash == *hash
                {
                    return Ok(tokens_equal(presented, &cached.token));
                }
                let ok = verify_password(presented, hash)?;
                if ok {
                    *self.lock() = Some(CachedToken {
                        token: presented.to_string(),
                        password_hash: hash.clone(),
                        updated_at,
                    });
                }
                Ok(ok)
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<CachedToken>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
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
    /// `new_token` contains characters that are not allowed.
    #[error(
        "new token must be printable ASCII without whitespace, quotes, semicolons, commas or backslashes"
    )]
    InvalidChars,
    /// `new_token` equals `old_token`.
    #[error("new token must differ from the old token")]
    SameAsOld,
    /// Storage or hashing failure.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Validate a candidate new token against the length, charset, and "different
/// from old" rules. The initial bootstrap token (64 hex characters) already
/// satisfies these rules; they apply only when the user sets a new token.
///
/// # Errors
///
/// Returns a [`ChangePasswordError`] variant describing the failed rule.
pub fn validate_new_token(new_token: &str, old_token: &str) -> Result<(), ChangePasswordError> {
    if new_token.len() < MIN_NEW_TOKEN_LEN {
        return Err(ChangePasswordError::TooShort {
            min: MIN_NEW_TOKEN_LEN,
        });
    }
    if !new_token.chars().all(is_allowed_token_char) {
        return Err(ChangePasswordError::InvalidChars);
    }
    if tokens_equal(old_token, new_token) {
        return Err(ChangePasswordError::SameAsOld);
    }
    Ok(())
}

/// A new token may use printable ASCII (`0x21`–`0x7E`) except `"`, `;`, `,`
/// and `\`, which would break cookies or `Authorization` headers.
fn is_allowed_token_char(c: char) -> bool {
    c.is_ascii_graphic() && !matches!(c, '"' | ';' | ',' | '\\')
}

/// Change the admin token: write the argon2id hash and delete `admin.passwd`.
///
/// If the password file cannot be deleted, the database is left unchanged.
/// The caller must invalidate any in-memory [`TokenCache`].
///
/// # Errors
///
/// Returns an error on validation or I/O failure. The caller maps these to HTTP.
pub fn change_password(
    paths: &DataPaths,
    conn: &mut Connection,
    old_token: &str,
    new_token: &str,
) -> Result<(), ChangePasswordError> {
    let state = load_auth_state(conn)?;
    let credential = current_credential(paths, &state)?;
    if !verify_credential(old_token, &credential)? {
        return Err(ChangePasswordError::IncorrectOld);
    }
    validate_new_token(new_token, old_token)?;
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
    Ok(())
}

/// Format a `Set-Cookie` value that stores the login token (no `Secure`, no
/// `Max-Age`: a browser session cookie). The token itself does not expire.
#[must_use]
pub fn auth_cookie(token: &str) -> String {
    format!("{AUTH_COOKIE}={token}; HttpOnly; Path=/; SameSite=Lax")
}

/// Format a `Set-Cookie` value that clears the auth cookie.
#[must_use]
pub fn clear_auth_cookie() -> String {
    format!("{AUTH_COOKIE}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0")
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
        AUTH_COOKIE, ChangePasswordError, Credential, MIN_NEW_TOKEN_LEN, TokenCache, auth_cookie,
        change_password, clear_auth_cookie, ensure_admin_passwd, hash_password, load_auth_state,
        password_file_log_line, random_token, tokens_equal, validate_new_token, verify_password,
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

    #[test]
    fn validate_new_token_enforces_length_charset_and_difference() {
        let old = "aaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(old.len(), MIN_NEW_TOKEN_LEN);
        let too_short = "a".repeat(MIN_NEW_TOKEN_LEN - 1);
        assert!(matches!(
            validate_new_token(&too_short, old),
            Err(ChangePasswordError::TooShort { min: 24 })
        ));
        assert!(matches!(
            validate_new_token("aaaaaaaaaaaaaaaaaaaaaaa;", old),
            Err(ChangePasswordError::InvalidChars)
        ));
        assert!(matches!(
            validate_new_token("aaaaaaaaaaaaaaaaaaaaaaa ", old),
            Err(ChangePasswordError::InvalidChars)
        ));
        assert!(matches!(
            validate_new_token("aaaaaaaaaaaaaaaaaaaaaaa\"", old),
            Err(ChangePasswordError::InvalidChars)
        ));
        assert!(matches!(
            validate_new_token(old, old),
            Err(ChangePasswordError::SameAsOld)
        ));
        validate_new_token("bbbbbbbbbbbbbbbbbbbbbbbb", old).expect("ok");
        validate_new_token("changed-password-ok-24ch", old).expect("ok");
    }

    #[test]
    fn auth_cookie_is_session_cookie_without_max_age_or_secure() {
        let cookie = auth_cookie("tok");
        assert!(
            cookie.starts_with(&format!("{AUTH_COOKIE}=tok;")),
            "{cookie}"
        );
        assert!(cookie.contains("HttpOnly"), "{cookie}");
        assert!(cookie.contains("Path=/"), "{cookie}");
        assert!(cookie.contains("SameSite=Lax"), "{cookie}");
        let lower = cookie.to_ascii_lowercase();
        assert!(!lower.contains("secure"), "{cookie}");
        assert!(!lower.contains("max-age"), "{cookie}");
        assert!(!lower.contains("expires="), "{cookie}");
        let clear = clear_auth_cookie();
        assert!(clear.contains("Max-Age=0"), "{clear}");
    }

    #[test]
    fn token_cache_compares_cached_value_and_invalidates_on_hash_change() {
        let cache = TokenCache::new();
        let token = "twenty-four-chars-token!!";
        let hash = hash_password(token).expect("hash");
        assert!(
            cache
                .verify(token, &Credential::Hash(hash.clone()), 1)
                .expect("first")
        );
        assert!(
            !cache
                .verify(
                    "wrong-token-xxxxxxxxxxxx",
                    &Credential::Hash(hash.clone()),
                    1
                )
                .expect("wrong")
        );
        assert!(
            cache
                .verify(token, &Credential::Hash(hash.clone()), 1)
                .expect("cached")
        );
        cache.invalidate();
        let other = "another-twenty-four-tok!!";
        let hash2 = hash_password(other).expect("hash2");
        assert!(
            !cache
                .verify(token, &Credential::Hash(hash2.clone()), 2)
                .expect("stale")
        );
        assert!(
            cache
                .verify(other, &Credential::Hash(hash2), 2)
                .expect("new")
        );
    }

    #[test]
    fn token_cache_bootstrap_compares_against_the_file_token() {
        let cache = TokenCache::new();
        let expected = "bootstrap-token-value-24ch";
        let credential = Credential::Bootstrap(expected.to_string());
        assert!(cache.verify(expected, &credential, 0).expect("match"));
        assert!(!cache.verify("other", &credential, 0).expect("mismatch"));
    }

    #[test]
    fn change_password_rejects_bad_input_and_stores_hash() {
        let tmp = TempDir::new("change");
        let paths = tmp.paths();
        let old = ensure_admin_passwd(&paths).expect("bootstrap");
        let mut conn = crate::db::open_central(&paths).expect("db");

        let err = change_password(&paths, &mut conn, &old, "short").expect_err("short");
        assert!(matches!(err, ChangePasswordError::TooShort { min: 24 }));

        let err = change_password(&paths, &mut conn, &old, "aaaaaaaaaaaaaaaaaaaaaaa;")
            .expect_err("charset");
        assert!(matches!(err, ChangePasswordError::InvalidChars));

        let err = change_password(
            &paths,
            &mut conn,
            "not-the-old-token-at-all!!",
            "changed-password-ok-24ch",
        )
        .expect_err("old");
        assert!(matches!(err, ChangePasswordError::IncorrectOld));

        let new = "changed-password-ok-24ch";
        change_password(&paths, &mut conn, &old, new).expect("change");
        assert!(
            !paths.admin_passwd().exists(),
            "admin.passwd must be deleted"
        );
        let state = load_auth_state(&conn).expect("state");
        assert!(!state.must_change);
        let hash = state.password_hash.expect("hash");
        assert!(verify_password(new, &hash).expect("verify new"));
        assert!(!verify_password(&old, &hash).expect("verify old"));

        let err = change_password(&paths, &mut conn, new, new).expect_err("same");
        assert!(matches!(err, ChangePasswordError::SameAsOld));
    }
}
