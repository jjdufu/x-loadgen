//! SQL schema for the central database (v2 Hoppscotch workspace) and per-worker databases (v1).

/// Current `PRAGMA user_version` for the central catalog & run database.
pub const USER_VERSION: i32 = 2;

/// Current `PRAGMA user_version` for per-worker sample databases (`wNN.sqlite`).
pub const WORKER_USER_VERSION: i32 = 1;

/// Drop legacy v1 four-tier catalog tables when upgrading a v1 central database to v2.
pub const DROP_LEGACY_V1_CATALOG: &str = "
DROP TABLE IF EXISTS templates;
DROP TABLE IF EXISTS target_products;
DROP TABLE IF EXISTS target_tenants;
DROP TABLE IF EXISTS targets;
DROP TABLE IF EXISTS sessions;
";

/// Central database DDL applied on fresh creation or v1 -> v2 upgrade.
pub const CENTRAL_SCHEMA_V2: &str = "
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

CREATE TABLE IF NOT EXISTS collections (
  id               TEXT PRIMARY KEY,
  parent_id        TEXT REFERENCES collections(id) ON DELETE CASCADE,
  name             TEXT NOT NULL,
  sort_order       INTEGER NOT NULL DEFAULT 0,
  default_protocol TEXT NOT NULL DEFAULT 'http',
  auth_json        TEXT NOT NULL DEFAULT '{\"auth_type\":\"inherit\",\"auth_active\":true}',
  headers_json     TEXT NOT NULL DEFAULT '[]',
  variables_json   TEXT NOT NULL DEFAULT '[]',
  description      TEXT,
  created_at       INTEGER NOT NULL,
  updated_at       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_collections_parent ON collections(parent_id, sort_order);

CREATE TABLE IF NOT EXISTS requests (
  id              TEXT PRIMARY KEY,
  collection_id   TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
  name            TEXT NOT NULL,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  protocol        TEXT NOT NULL DEFAULT 'http',
  method          TEXT NOT NULL DEFAULT 'GET',
  endpoint        TEXT NOT NULL DEFAULT '',
  params_json     TEXT NOT NULL DEFAULT '[]',
  headers_json    TEXT NOT NULL DEFAULT '[]',
  body_json       TEXT NOT NULL DEFAULT '{\"content_type\":null,\"body\":null,\"form\":[]}',
  auth_json       TEXT NOT NULL DEFAULT '{\"auth_type\":\"inherit\",\"auth_active\":true}',
  mazbri_json     TEXT NOT NULL DEFAULT '{}',
  tls_json        TEXT NOT NULL DEFAULT '{}',
  assertions_json TEXT NOT NULL DEFAULT '{\"expect_transport\":\"ok\",\"checks\":[]}',
  weight          INTEGER NOT NULL DEFAULT 1,
  timeout_secs    INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_requests_collection ON requests(collection_id, sort_order);

CREATE TABLE IF NOT EXISTS environments (
  id               TEXT PRIMARY KEY,
  name             TEXT NOT NULL UNIQUE,
  is_global        INTEGER NOT NULL DEFAULT 0,
  gateway_base_url TEXT,
  variables_json   TEXT NOT NULL DEFAULT '[]',
  http_auth_json   TEXT NOT NULL DEFAULT '{\"auth_type\":\"none\",\"auth_active\":true}',
  tls_json         TEXT NOT NULL DEFAULT '{}',
  note             TEXT,
  created_at       INTEGER NOT NULL,
  updated_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS environment_tenants (
  id                 TEXT PRIMARY KEY,
  environment_id     TEXT NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
  name               TEXT NOT NULL,
  weight             INTEGER NOT NULL DEFAULT 1,
  api_key            TEXT,
  encryption_mode    TEXT NOT NULL DEFAULT 'none',
  tenant_id          TEXT,
  tenant_key_base64  TEXT,
  key_version        TEXT,
  suite              TEXT,
  data_alg           TEXT,
  mac_alg            TEXT,
  require_signature  INTEGER,
  allow_insecure     INTEGER NOT NULL DEFAULT 0,
  binding_kid        TEXT,
  binding_alg        TEXT,
  binding_key        TEXT,
  binding_public_key TEXT,
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL,
  UNIQUE(environment_id, name)
);
CREATE INDEX IF NOT EXISTS idx_env_tenants_env ON environment_tenants(environment_id);

CREATE TABLE IF NOT EXISTS load_profiles (
  id                      TEXT PRIMARY KEY,
  name                    TEXT NOT NULL UNIQUE,
  model                   TEXT NOT NULL DEFAULT 'open_loop',
  qps                     INTEGER,
  concurrency             INTEGER NOT NULL DEFAULT 10,
  requests                INTEGER,
  duration_secs           INTEGER,
  warmup_secs             INTEGER NOT NULL DEFAULT 0,
  ramp_secs               INTEGER NOT NULL DEFAULT 0,
  timeout_secs            INTEGER NOT NULL DEFAULT 30,
  connect_timeout_secs    INTEGER NOT NULL DEFAULT 5,
  token_refresh_skew_secs INTEGER NOT NULL DEFAULT 60,
  include_request_id      INTEGER NOT NULL DEFAULT 1,
  processes               INTEGER NOT NULL DEFAULT 1,
  qps_mode                TEXT NOT NULL DEFAULT 'total',
  requests_mode           TEXT NOT NULL DEFAULT 'total',
  loop_enabled            INTEGER NOT NULL DEFAULT 0,
  rounds                  INTEGER,
  loop_interval_secs      INTEGER NOT NULL DEFAULT 0,
  expected_reasons_json   TEXT NOT NULL DEFAULT '[]',
  detail_retain_days      INTEGER,
  detail_retain_rows      INTEGER,
  assert_json             TEXT,
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
