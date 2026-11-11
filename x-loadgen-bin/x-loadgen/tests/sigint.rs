//! SIGINT against a real `serve` process stops running tasks and exits.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::{Value, json};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-sigint-{}-{}-{n}-{tag}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        ));
        fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client")
}

async fn body(response: reqwest::Response) -> Value {
    response.json::<Value>().await.expect("json")
}

async fn post_json(
    http: &reqwest::Client,
    url: &str,
    payload: &Value,
) -> (reqwest::StatusCode, Value) {
    let response = http.post(url).json(payload).send().await.expect("post");
    let status = response.status();
    (status, body(response).await)
}

async fn start_mock() -> (String, tokio::sync::oneshot::Sender<()>) {
    use axum::Router;
    use axum::routing::get;
    let app = Router::new().route("/ok", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock");
    let addr = listener.local_addr().expect("addr");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .expect("mock server");
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    (format!("http://{addr}"), tx)
}

fn target_payload(gateway: &str) -> Value {
    json!({
        "name": "gw-sigint",
        "gateway_base_url": gateway,
        "insecure_skip_verify": true,
        "tenants": [{
            "name": "plain",
            "weight": 1,
            "api_key": "k",
            "encryption_mode": "none"
        }],
        "products": [{
            "name": "probe",
            "product_id": "P001",
            "path": "/ok",
            "method": "GET",
            "weight": 1
        }]
    })
}

fn once_template(target_id: &str) -> Value {
    json!({
        "name": "sigint-once",
        "target_id": target_id,
        "model": "open_loop",
        "qps": 8,
        "concurrency": 2,
        "requests": 10_000,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "processes": 1,
        "loop_enabled": false
    })
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .expect("addr")
        .port()
}

async fn wait_until_healthy(http: &reqwest::Client, base: &str, log: &Path) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if http
            .get(format!("{base}/health"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return;
        }
        let log_text = fs::read_to_string(log).unwrap_or_default();
        assert!(
            tokio::time::Instant::now() < deadline,
            "serve did not become healthy on {base}: {log_text}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn wait_for_exit(child: &mut Child) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "serve did not exit after SIGINT"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => panic!("wait: {err}"),
        }
    }
}

#[tokio::test]
async fn sigint_stops_running_tasks_and_exits() {
    let tmp = TempDir::new("sigint");
    let (mock, _mock_shutdown) = start_mock().await;
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    assert!(bin.exists(), "missing binary {}", bin.display());
    let log_path = tmp.path.join("serve.log");
    let log = fs::File::create(&log_path).expect("serve log");
    let mut child = Command::new(&bin)
        .arg("serve")
        .arg("--no-auth")
        .arg("--listen")
        .arg(&listen)
        .arg("--data-dir")
        .arg(&tmp.path)
        .stdout(Stdio::from(log.try_clone().expect("clone log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn serve");

    let base = format!("http://{listen}");
    let http = client();
    wait_until_healthy(&http, &base, &log_path).await;

    let (status, target) = post_json(
        &http,
        &format!("{base}/api/v1/targets"),
        &target_payload(&mock),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let (status, template) = post_json(
        &http,
        &format!("{base}/api/v1/templates"),
        &once_template(target["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    let (status, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template["id"], "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();
    assert_eq!(run["status"], "running");

    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(killed.success(), "kill -INT failed: {killed}");
    wait_for_exit(&mut child);

    let conn = Connection::open(tmp.path.join("loadgen.sqlite")).expect("open central");
    let status: String = conn
        .query_row(
            "SELECT status FROM runs WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .expect("run status");
    assert!(
        matches!(status.as_str(), "interrupted" | "stopped" | "finished"),
        "unexpected status {status}"
    );
    let run_dir = tmp.path.join("runs").join(&id);
    assert!(
        run_dir.is_dir(),
        "run directory should exist after shutdown: {}",
        run_dir.display()
    );
}
