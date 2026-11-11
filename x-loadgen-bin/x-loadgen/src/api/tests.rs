//! HTTP-level tests for health, login, password change, and `--no-auth`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;

use super::server::{prepare, spawn};
use crate::auth::{SESSION_COOKIE, read_admin_passwd, reset_passwd};
use crate::cli::ServeArgs;
use crate::db::DataPaths;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-http-{}-{}-{n}-{tag}",
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

fn serve_args(dir: &std::path::Path, no_auth: bool) -> ServeArgs {
    ServeArgs {
        listen: "127.0.0.1:0".to_string(),
        data_dir: dir.to_path_buf(),
        no_auth,
        max_qps: None,
        max_concurrency: None,
    }
}

async fn start(dir: &std::path::Path, no_auth: bool) -> (String, tokio::sync::oneshot::Sender<()>) {
    let prepared = prepare(serve_args(dir, no_auth)).expect("prepare");
    let (addr, shutdown, _join) = spawn(prepared).await.expect("spawn");
    (format!("http://{addr}"), shutdown)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client")
}

async fn json_body(response: reqwest::Response) -> Value {
    response.json::<Value>().await.expect("json")
}

async fn login_with(
    client: &reqwest::Client,
    base: &str,
    token: &str,
) -> (reqwest::StatusCode, Option<String>, Value) {
    let response = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({"token": token}))
        .send()
        .await
        .expect("login");
    let status = response.status();
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    (status, cookie, json_body(response).await)
}

fn assert_session_cookie(set_cookie: &str) {
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("Path=/"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert!(
        !set_cookie.to_ascii_lowercase().contains("secure"),
        "{set_cookie}"
    );
    assert!(
        set_cookie.contains(&format!("{SESSION_COOKIE}=")),
        "{set_cookie}"
    );
}

#[tokio::test]
async fn serve_creates_password_file_mode_600_and_does_not_overwrite() {
    let tmp = TempDir::new("file");
    let (base, shutdown) = start(&tmp.path, false).await;
    let path = tmp.paths().admin_passwd();
    assert!(path.is_file());
    let mode = fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let first = read_admin_passwd(&path).expect("read");
    shutdown.send(()).ok();

    let prepared = prepare(serve_args(&tmp.path, false)).expect("prepare again");
    drop(prepared);
    let again = read_admin_passwd(&path).expect("read again");
    assert_eq!(first, again);

    let health = client().get(format!("{base}/health")).send().await;
    // server may already be shut down; the file assertions are the point of this test
    let _ = health;
}

#[tokio::test]
async fn login_cookie_and_bearer_and_password_change() {
    let tmp = TempDir::new("login");
    let (base, shutdown) = start(&tmp.path, false).await;
    let client = client();
    let file_token = read_admin_passwd(&tmp.paths().admin_passwd()).expect("file token");

    let health = client
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("health");
    assert!(health.status().is_success());
    let health_json = json_body(health).await;
    assert_eq!(health_json["ok"], true);
    assert_eq!(health_json["version"], env!("CARGO_PKG_VERSION"));

    let (status, set_cookie, body) = login_with(&client, &base, &file_token).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let set_cookie = set_cookie.expect("set-cookie");
    assert_session_cookie(&set_cookie);
    assert_eq!(body["must_change_password"], true);
    let session = body["token"].as_str().expect("session token").to_string();
    let cookie_header = format!("{SESSION_COOKIE}={session}");

    let forbidden = client
        .get(format!("{base}/api/v1/meta"))
        .header(reqwest::header::COOKIE, &cookie_header)
        .send()
        .await
        .expect("meta before change");
    assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);
    let forbidden_json = json_body(forbidden).await;
    assert_eq!(forbidden_json["error"]["code"], "password_change_required");

    let me = client
        .get(format!("{base}/api/v1/auth/me"))
        .header(reqwest::header::COOKIE, &cookie_header)
        .send()
        .await
        .expect("me");
    assert!(me.status().is_success());
    let me_json = json_body(me).await;
    assert_eq!(me_json["auth_required"], true);
    assert_eq!(me_json["must_change_password"], true);

    let new_token = "changed-password-1";
    let change = client
        .post(format!("{base}/api/v1/auth/password"))
        .header(reqwest::header::COOKIE, &cookie_header)
        .json(&serde_json::json!({"old_token": file_token, "new_token": new_token}))
        .send()
        .await
        .expect("change");
    assert!(
        change.status().is_success(),
        "change failed: {:?}",
        change.status()
    );
    assert!(
        !tmp.paths().admin_passwd().exists(),
        "admin.passwd should be deleted"
    );

    let (old_status, _, _) = login_with(&client, &base, &file_token).await;
    assert_eq!(old_status, reqwest::StatusCode::UNAUTHORIZED);

    let (new_status, _, new_body) = login_with(&client, &base, new_token).await;
    assert!(new_status.is_success());
    assert_eq!(new_body["must_change_password"], false);
    let new_session = new_body["token"].as_str().expect("token").to_string();

    let via_cookie = client
        .get(format!("{base}/api/v1/meta"))
        .header(
            reqwest::header::COOKIE,
            format!("{SESSION_COOKIE}={new_session}"),
        )
        .send()
        .await
        .expect("meta cookie");
    assert!(via_cookie.status().is_success());

    let via_bearer = client
        .get(format!("{base}/api/v1/meta"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {new_session}"),
        )
        .send()
        .await
        .expect("meta bearer");
    assert!(via_bearer.status().is_success());
    let meta = json_body(via_bearer).await;
    assert_eq!(meta["auth_required"], true);

    shutdown.send(()).ok();
}

#[tokio::test]
async fn reset_passwd_revokes_old_secret_and_requires_change_again() {
    let tmp = TempDir::new("reset");
    let (base, shutdown) = start(&tmp.path, false).await;
    let client = client();
    let original = read_admin_passwd(&tmp.paths().admin_passwd()).expect("original");
    let login = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({"token": original}))
        .send()
        .await
        .expect("login");
    let session = json_body(login).await["token"]
        .as_str()
        .expect("session")
        .to_string();

    reset_passwd(&tmp.paths()).expect("reset");
    let regenerated = read_admin_passwd(&tmp.paths().admin_passwd()).expect("new file");
    assert_ne!(regenerated, original);

    let stale = client
        .get(format!("{base}/api/v1/auth/me"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {session}"))
        .send()
        .await
        .expect("stale session");
    assert_eq!(stale.status(), reqwest::StatusCode::UNAUTHORIZED);

    let old_secret = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({"token": original}))
        .send()
        .await
        .expect("old secret");
    assert_eq!(old_secret.status(), reqwest::StatusCode::UNAUTHORIZED);

    let fresh = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({"token": regenerated}))
        .send()
        .await
        .expect("new secret");
    assert!(fresh.status().is_success());
    let fresh_body = json_body(fresh).await;
    assert_eq!(fresh_body["must_change_password"], true);

    shutdown.send(()).ok();
}

#[tokio::test]
async fn no_auth_allows_protected_routes_without_headers_and_skips_password_file() {
    let tmp = TempDir::new("noauth");
    let (base, shutdown) = start(&tmp.path, true).await;
    assert!(
        !tmp.paths().admin_passwd().exists(),
        "no-auth must not create admin.passwd"
    );
    let client = client();
    let meta = client
        .get(format!("{base}/api/v1/meta"))
        .send()
        .await
        .expect("meta");
    assert!(meta.status().is_success());
    let body = json_body(meta).await;
    assert_eq!(body["auth_required"], false);

    let me = client
        .get(format!("{base}/api/v1/auth/me"))
        .send()
        .await
        .expect("me");
    assert!(me.status().is_success());
    let me_body = json_body(me).await;
    assert_eq!(me_body["auth_required"], false);
    assert_eq!(me_body["must_change_password"], false);

    shutdown.send(()).ok();
}

#[test]
fn unwritable_data_dir_fails_before_listening() {
    let tmp = TempDir::new("blocked");
    let blocker = tmp.path.join("not-a-dir");
    fs::write(&blocker, b"file").expect("blocker");
    let args = serve_args(&blocker.join("data"), false);
    let Err(err) = prepare(args) else {
        panic!("prepare should fail");
    };
    let text = format!("{err:#}");
    assert!(
        text.contains("not writable") || text.contains(blocker.to_string_lossy().as_ref()),
        "{text}"
    );
}

#[tokio::test]
async fn root_serves_embedded_spa_or_placeholder() {
    let tmp = TempDir::new("spa");
    let (base, shutdown) = start(&tmp.path, true).await;
    let http = client();
    let response = http.get(format!("{base}/")).send().await.expect("get /");
    let status = response.status();
    let text = response.text().await.expect("body");
    if cfg!(frontend_embedded) {
        assert_eq!(status, reqwest::StatusCode::OK, "{text}");
        assert!(
            text.contains("<div id=\"app\"") || text.contains("<div id='app'"),
            "expected SPA shell: {text:.200}"
        );
    } else {
        assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE, "{text}");
        assert!(text.contains("前端未嵌入"), "{text}");
    }
    let health = http
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("health");
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    let _ = shutdown.send(());
}
