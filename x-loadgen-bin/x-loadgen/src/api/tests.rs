//! HTTP-level tests for health, login, password change, and `--no-auth`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;

use super::server::{prepare, spawn};
use crate::auth::{AUTH_COOKIE, MIN_NEW_TOKEN_LEN, read_admin_passwd, reset_passwd};
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

fn assert_auth_cookie(set_cookie: &str, token: &str) {
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("Path=/"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    let lower = set_cookie.to_ascii_lowercase();
    assert!(!lower.contains("secure"), "{set_cookie}");
    assert!(
        !lower.contains("max-age") && !lower.contains("expires="),
        "login cookie must not auto-expire: {set_cookie}"
    );
    assert!(
        set_cookie.contains(&format!("{AUTH_COOKIE}={token}")),
        "{set_cookie}"
    );
}

fn cookie_for(token: &str) -> String {
    format!("{AUTH_COOKIE}={token}")
}

async fn post_password(
    client: &reqwest::Client,
    base: &str,
    presented: &str,
    old: &str,
    new: &str,
) -> (reqwest::StatusCode, Option<String>, Value) {
    let response = client
        .post(format!("{base}/api/v1/auth/password"))
        .header(reqwest::header::COOKIE, cookie_for(presented))
        .json(&serde_json::json!({"old_token": old, "new_token": new}))
        .send()
        .await
        .expect("password");
    let status = response.status();
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    (status, cookie, json_body(response).await)
}

async fn get_auth(
    client: &reqwest::Client,
    url: &str,
    presented: Option<&str>,
    as_bearer: bool,
) -> (reqwest::StatusCode, Value) {
    let mut req = client.get(url);
    if let Some(token) = presented {
        req = if as_bearer {
            req.header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        } else {
            req.header(reqwest::header::COOKIE, cookie_for(token))
        };
    }
    let response = req.send().await.expect("get");
    (response.status(), json_body(response).await)
}

const NEW_TOKEN: &str = "changed-password-ok-24ch";
const NEW_TOKEN_RESTART: &str = "changed-password-restart1";

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
async fn login_echoes_token_in_cookie_and_blocks_meta_until_change() {
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

    // Bearer with the login token works without calling login first.
    let (me_status, me_json) = get_auth(
        &client,
        &format!("{base}/api/v1/auth/me"),
        Some(&file_token),
        true,
    )
    .await;
    assert!(me_status.is_success());
    assert_eq!(me_json["auth_required"], true);
    assert_eq!(me_json["must_change_password"], true);

    let (status, set_cookie, body) = login_with(&client, &base, &file_token).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let set_cookie = set_cookie.expect("set-cookie");
    assert_auth_cookie(&set_cookie, &file_token);
    assert_eq!(body["must_change_password"], true);
    assert_eq!(body["token"].as_str().expect("token"), file_token);

    let (forbidden_status, forbidden_json) = get_auth(
        &client,
        &format!("{base}/api/v1/meta"),
        Some(&file_token),
        false,
    )
    .await;
    assert_eq!(forbidden_status, reqwest::StatusCode::FORBIDDEN);
    assert_eq!(forbidden_json["error"]["code"], "password_change_required");

    let (me_status, me_json) = get_auth(
        &client,
        &format!("{base}/api/v1/auth/me"),
        Some(&file_token),
        false,
    )
    .await;
    assert!(me_status.is_success());
    assert_eq!(me_json["auth_required"], true);
    assert_eq!(me_json["must_change_password"], true);

    shutdown.send(()).ok();
}

#[tokio::test]
async fn password_change_rejects_weak_tokens_then_revokes_the_old_one() {
    let tmp = TempDir::new("change");
    let (base, shutdown) = start(&tmp.path, false).await;
    let client = client();
    let file_token = read_admin_passwd(&tmp.paths().admin_passwd()).expect("file token");
    let (status, _, _) = login_with(&client, &base, &file_token).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    let too_short = "a".repeat(MIN_NEW_TOKEN_LEN - 1);
    let (short_status, _, short_json) =
        post_password(&client, &base, &file_token, &file_token, &too_short).await;
    assert_eq!(short_status, reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(short_json["error"]["code"], "invalid_request");
    assert!(
        short_json["error"]["message"]
            .as_str()
            .expect("msg")
            .contains("24"),
        "{short_json}"
    );

    let (invalid_status, _, _) = post_password(
        &client,
        &base,
        &file_token,
        &file_token,
        "aaaaaaaaaaaaaaaaaaaaaaa;",
    )
    .await;
    assert_eq!(invalid_status, reqwest::StatusCode::BAD_REQUEST);

    let (change_status, change_cookie, _) =
        post_password(&client, &base, &file_token, &file_token, NEW_TOKEN).await;
    assert!(
        change_status.is_success(),
        "change failed: {change_status:?}"
    );
    assert_auth_cookie(&change_cookie.expect("change set-cookie"), NEW_TOKEN);
    assert!(
        !tmp.paths().admin_passwd().exists(),
        "admin.passwd should be deleted"
    );

    let (stale_status, _) = get_auth(
        &client,
        &format!("{base}/api/v1/auth/me"),
        Some(&file_token),
        false,
    )
    .await;
    assert_eq!(stale_status, reqwest::StatusCode::UNAUTHORIZED);

    let (old_status, _, _) = login_with(&client, &base, &file_token).await;
    assert_eq!(old_status, reqwest::StatusCode::UNAUTHORIZED);

    let (meta_status, meta) = get_auth(
        &client,
        &format!("{base}/api/v1/meta"),
        Some(NEW_TOKEN),
        true,
    )
    .await;
    assert!(meta_status.is_success());
    assert_eq!(meta["auth_required"], true);

    let (cookie_status, _) = get_auth(
        &client,
        &format!("{base}/api/v1/meta"),
        Some(NEW_TOKEN),
        false,
    )
    .await;
    assert!(cookie_status.is_success());

    let (new_status, _, new_body) = login_with(&client, &base, NEW_TOKEN).await;
    assert!(new_status.is_success());
    assert_eq!(new_body["must_change_password"], false);
    assert_eq!(new_body["token"].as_str().expect("token"), NEW_TOKEN);

    shutdown.send(()).ok();
}

#[tokio::test]
async fn restart_after_password_change_does_not_recreate_password_file() {
    let tmp = TempDir::new("restart");
    let (base, shutdown) = start(&tmp.path, false).await;
    let client = client();
    let file_token = read_admin_passwd(&tmp.paths().admin_passwd()).expect("file token");
    let (status, _, body) = login_with(&client, &base, &file_token).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let token = body["token"].as_str().expect("token").to_string();
    let change = client
        .post(format!("{base}/api/v1/auth/password"))
        .header(reqwest::header::COOKIE, format!("{AUTH_COOKIE}={token}"))
        .json(&serde_json::json!({"old_token": file_token, "new_token": NEW_TOKEN_RESTART}))
        .send()
        .await
        .expect("change");
    assert!(change.status().is_success());
    shutdown.send(()).ok();

    let (base, shutdown) = start(&tmp.path, false).await;
    assert!(
        !tmp.paths().admin_passwd().exists(),
        "restart must not recreate admin.passwd once a password hash is stored"
    );
    // The same login token is accepted after a process restart, without a
    // prior /login call (the in-memory cache starts empty and argon2 runs once).
    let via_bearer = client
        .get(format!("{base}/api/v1/meta"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {NEW_TOKEN_RESTART}"),
        )
        .send()
        .await
        .expect("restart bearer");
    assert!(via_bearer.status().is_success());
    let (new_status, _, new_body) = login_with(&client, &base, NEW_TOKEN_RESTART).await;
    assert!(new_status.is_success());
    assert_eq!(new_body["must_change_password"], false);
    shutdown.send(()).ok();
}

#[tokio::test]
async fn reset_passwd_revokes_old_secret_and_requires_change_again() {
    let tmp = TempDir::new("reset");
    let (base, shutdown) = start(&tmp.path, false).await;
    let client = client();
    let original = read_admin_passwd(&tmp.paths().admin_passwd()).expect("original");
    // Warm the path (and the cache, once a hash exists) by changing first.
    let login = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({"token": original}))
        .send()
        .await
        .expect("login");
    assert!(login.status().is_success());
    let change = client
        .post(format!("{base}/api/v1/auth/password"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {original}"))
        .json(&serde_json::json!({"old_token": original, "new_token": NEW_TOKEN}))
        .send()
        .await
        .expect("change");
    assert!(change.status().is_success());
    // Warm the argon2 cache with a successful request.
    let warm = client
        .get(format!("{base}/api/v1/meta"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {NEW_TOKEN}"),
        )
        .send()
        .await
        .expect("warm");
    assert!(warm.status().is_success());

    reset_passwd(&tmp.paths()).expect("reset");
    let regenerated = read_admin_passwd(&tmp.paths().admin_passwd()).expect("new file");
    assert_ne!(regenerated, original);
    assert_ne!(regenerated, NEW_TOKEN);

    // The running server must notice the out-of-process reset on the next
    // request, even though its in-memory cache still holds the previous token.
    let stale = client
        .get(format!("{base}/api/v1/auth/me"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {NEW_TOKEN}"),
        )
        .send()
        .await
        .expect("stale token");
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
    assert_eq!(
        fresh_body["token"].as_str().expect("token"),
        regenerated.as_str()
    );

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
