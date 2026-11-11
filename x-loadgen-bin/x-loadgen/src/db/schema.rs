//! SQL schema for the central database and per-worker databases.

/// Current `PRAGMA user_version` for both database kinds. Migrations only move forward.
pub const USER_VERSION: i32 = 1;

/// Central database DDL applied when `user_version` is 0.
pub const CENTRAL_SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_state (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  password_hash TEXT,
  must_change   INTEGER NOT NULL DEFAULT 1,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  id           TEXT PRIMARY KEY,
  token_hash   TEXT NOT NULL UNIQUE,
  created_at   INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS targets (
  id                   TEXT PRIMARY KEY,
  name                 TEXT NOT NULL UNIQUE,
  gateway_base_url     TEXT NOT NULL,
  client_cert          TEXT,
  client_key           TEXT,
  ca_cert              TEXT,
  insecure_skip_verify INTEGER NOT NULL DEFAULT 0,
  note                 TEXT,
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS target_tenants (
  id                TEXT PRIMARY KEY,
  target_id         TEXT NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
  name              TEXT NOT NULL,
  weight            INTEGER NOT NULL DEFAULT 1,
  api_key           TEXT NOT NULL,
  encryption_mode   TEXT NOT NULL,
  tenant_id         TEXT,
  tenant_key_base64 TEXT,
  key_version       TEXT,
  suite             TEXT,
  data_alg          TEXT,
  mac_alg           TEXT,
  require_signature INTEGER,
  allow_insecure    INTEGER NOT NULL DEFAULT 0,
  binding_kid       TEXT,
  binding_alg       TEXT,
  binding_key       TEXT,
  UNIQUE(target_id, name)
);

CREATE TABLE IF NOT EXISTS target_products (
  id                    TEXT PRIMARY KEY,
  target_id             TEXT NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
  name                  TEXT NOT NULL,
  product_id            TEXT NOT NULL,
  channel_code          TEXT,
  path                  TEXT NOT NULL,
  method                TEXT NOT NULL,
  weight                INTEGER NOT NULL DEFAULT 1,
  qps_limit             INTEGER,
  timeout_secs          INTEGER,
  query_json            TEXT,
  headers_json          TEXT,
  body_json             TEXT,
  body_image_dir        TEXT,
  body_image_field      TEXT,
  body_image_name_field TEXT,
  expect_transport      TEXT,
  expect_checks_json    TEXT,
  UNIQUE(target_id, name)
);

CREATE TABLE IF NOT EXISTS templates (
  id                      TEXT PRIMARY KEY,
  name                    TEXT NOT NULL UNIQUE,
  target_id               TEXT NOT NULL REFERENCES targets(id),
  model                   TEXT NOT NULL,
  qps                     INTEGER,
  concurrency             INTEGER NOT NULL,
  requests                INTEGER,
  duration_secs           INTEGER,
  warmup_secs             INTEGER NOT NULL DEFAULT 0,
  ramp_secs               INTEGER NOT NULL DEFAULT 0,
  timeout_secs            INTEGER NOT NULL,
  connect_timeout_secs    INTEGER NOT NULL DEFAULT 5,
  token_refresh_skew_secs INTEGER NOT NULL DEFAULT 60,
  include_request_id      INTEGER NOT NULL DEFAULT 1,
  processes               INTEGER NOT NULL DEFAULT 1,
  qps_mode                TEXT NOT NULL DEFAULT 'total',
  requests_mode           TEXT NOT NULL DEFAULT 'total',
  loop_enabled            INTEGER NOT NULL DEFAULT 0,
  rounds                  INTEGER,
  loop_interval_secs      INTEGER NOT NULL DEFAULT 0,
  expected_reasons_json   TEXT,
  detail_retain_days      INTEGER,
  detail_retain_rows      INTEGER,
  product_filter_json     TEXT,
  note                    TEXT,
  created_at              INTEGER NOT NULL,
  updated_at              INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  id                    TEXT PRIMARY KEY,
  template_id           TEXT NOT NULL,
  target_id             TEXT NOT NULL,
  mode                  TEXT NOT NULL,
  status                TEXT NOT NULL,
  started_at            INTEGER,
  ended_at              INTEGER,
  current_qps           INTEGER,
  current_concurrency   INTEGER,
  processes             INTEGER NOT NULL,
  snapshot_json         TEXT NOT NULL,
  interrupt_ranges_json TEXT,
  error                 TEXT,
  created_at            INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS run_control (
  run_id      TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
  stop_flag   INTEGER NOT NULL DEFAULT 0,
  qps         INTEGER,
  concurrency INTEGER,
  epoch       INTEGER NOT NULL DEFAULT 0,
  updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS run_timeline (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id       TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  ts           INTEGER NOT NULL,
  event        TEXT NOT NULL,
  payload_json TEXT
);
CREATE INDEX IF NOT EXISTS idx_timeline_run_ts ON run_timeline(run_id, ts);

CREATE TABLE IF NOT EXISTS run_windows (
  run_id           TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  window_index     INTEGER NOT NULL,
  started_at       INTEGER NOT NULL,
  ended_at         INTEGER,
  samples          INTEGER NOT NULL DEFAULT 0,
  success          INTEGER NOT NULL DEFAULT 0,
  failure          INTEGER NOT NULL DEFAULT 0,
  expected         INTEGER NOT NULL DEFAULT 0,
  dropped_details  INTEGER NOT NULL DEFAULT 0,
  p50_us           INTEGER,
  p95_us           INTEGER,
  p99_us           INTEGER,
  max_us           INTEGER,
  bytes            INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (run_id, window_index)
);

CREATE TABLE IF NOT EXISTS run_checkpoints (
  run_id         TEXT NOT NULL,
  worker_index   INTEGER NOT NULL,
  ts             INTEGER NOT NULL,
  window_index   INTEGER,
  counters_json  TEXT NOT NULL,
  histogram_blob BLOB,
  PRIMARY KEY (run_id, worker_index, ts)
);
";

/// Worker database DDL applied when `user_version` is 0.
pub const WORKER_SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS worker_meta (
  run_id       TEXT NOT NULL,
  worker_index INTEGER NOT NULL,
  started_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS samples (
  seq            INTEGER PRIMARY KEY,
  start_unix_ms  INTEGER NOT NULL,
  end_unix_ms    INTEGER NOT NULL,
  latency_us     INTEGER NOT NULL,
  product_id     TEXT NOT NULL,
  product_name   TEXT,
  request_index  INTEGER,
  http_status    INTEGER,
  success        INTEGER NOT NULL,
  expected       INTEGER NOT NULL,
  reason         TEXT,
  error_class    TEXT,
  request_id     TEXT,
  error          TEXT,
  response_bytes INTEGER,
  customer_name  TEXT
);
CREATE INDEX IF NOT EXISTS idx_samples_end ON samples(end_unix_ms);
CREATE INDEX IF NOT EXISTS idx_samples_product ON samples(product_id, end_unix_ms);

CREATE TABLE IF NOT EXISTS worker_checkpoints (
  ts              INTEGER PRIMARY KEY,
  window_index    INTEGER,
  counters_json   TEXT NOT NULL,
  histogram_blob  BLOB,
  dropped_details INTEGER NOT NULL DEFAULT 0
);
";
