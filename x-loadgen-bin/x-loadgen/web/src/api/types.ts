export interface MeResponse {
  auth_required: boolean;
  must_change_password: boolean;
}

export interface MetaResponse {
  version: string;
  auth_required: boolean;
  max_qps: number | null;
  max_concurrency: number | null;
  running_qps: number;
  running_concurrency: number;
}

export interface Tenant {
  id?: string;
  target_id?: string;
  name: string;
  weight: number;
  api_key?: string;
  encryption_mode: string;
  tenant_id?: string | null;
  tenant_key_base64?: string | null;
  key_version?: string | null;
  suite?: string | null;
  data_alg?: string | null;
  mac_alg?: string | null;
  require_signature?: boolean | null;
  allow_insecure?: boolean;
  binding_kid?: string | null;
  binding_alg?: string | null;
  binding_key?: string | null;
  binding_public_key?: string | null;
}

export type TargetProtocol = "http" | "mazbri";

export interface KeypairResponse {
  binding_kid: string | null;
  binding_alg: string | null;
  public_key_pem: string;
}

export interface Product {
  id?: string;
  target_id?: string;
  name: string;
  product_id: string;
  path: string;
  method: string;
  weight: number;
  qps_limit?: number | null;
  timeout_secs?: number | null;
  query_json?: unknown;
  headers_json?: unknown;
  body_json?: unknown;
  expect_transport?: string | null;
  expect_checks_json?: unknown;
}

export interface TargetSummary {
  id: string;
  name: string;
  protocol: TargetProtocol;
  gateway_base_url: string;
  insecure_skip_verify: boolean;
  note?: string | null;
  created_at: number;
  updated_at: number;
}

export interface TargetDetail extends TargetSummary {
  client_cert?: string | null;
  client_key?: string | null;
  ca_cert?: string | null;
  tenants: Tenant[];
  products: Product[];
}

export interface Template {
  id: string;
  name: string;
  target_id: string;
  model: string;
  qps: number | null;
  concurrency: number;
  requests: number | null;
  duration_secs: number | null;
  warmup_secs: number;
  ramp_secs: number;
  timeout_secs: number;
  connect_timeout_secs: number;
  token_refresh_skew_secs: number;
  include_request_id: boolean;
  processes: number;
  qps_mode: string;
  requests_mode: string;
  loop_enabled: boolean;
  rounds: number | null;
  loop_interval_secs: number;
  expected_reasons: string[] | null;
  detail_retain_days: number | null;
  detail_retain_rows: number | null;
  product_ids: string[] | null;
  weight_overrides: Record<string, number> | null;
  note: string | null;
  created_at: number;
  updated_at: number;
}


export interface AssertSpec {
  p50_ms?: number | null;
  p95_ms?: number | null;
  p99_ms?: number | null;
  max_ms?: number | null;
  max_error_rate?: number | null;
  min_qps_ratio?: number | null;
}

export interface FailedCheck {
  check: string;
  wanted: number;
  actual?: number | null;
}

export interface ActualMetrics {
  samples: number;
  success: number;
  failure: number;
  p50_ms?: number | null;
  p95_ms?: number | null;
  p99_ms?: number | null;
  max_ms?: number | null;
  error_rate?: number | null;
  achieved_qps?: number | null;
  qps_ratio?: number | null;
}

export interface Verdict {
  result: "pass" | "fail" | "pending" | "none";
  assert?: AssertSpec;
  actual?: ActualMetrics;
  failed?: FailedCheck[];
}

export interface RunView {
  id: string;
  template_id: string;
  target_id: string;
  mode: string;
  status: string;
  started_at: number | null;
  ended_at: number | null;
  current_qps: number | null;
  current_concurrency: number | null;
  processes: number;
  interrupt_ranges: { start: number; end: number }[] | null;
  error: string | null;
  created_at: number;
  snapshot?: unknown;
  verdict?: Verdict | null;
}

export interface RunStats {
  id: string;
  status: string;
  current_qps: number | null;
  current_concurrency: number | null;
  samples: number;
  success: number;
  failure: number;
  expected: number;
  bytes: number;
  dropped_details: number;
  in_flight: number;
  peak_in_flight: number;
  p50_us: number | null;
  p95_us: number | null;
  p99_us: number | null;
  max_us: number | null;
  window_index: number | null;
}

export interface TimePoint {
  unix_sec?: number;
  started_at?: number;
  samples: number;
  success: number;
  failure: number;
  expected?: number;
  bytes?: number;
  warmup?: boolean;
}

export interface TimelineEvent {
  id: number;
  run_id: string;
  ts: number;
  event: string;
  payload?: unknown;
}

export interface WindowRow {
  window_index: number;
  started_at: number;
  ended_at: number | null;
  samples: number;
  success: number;
  failure: number;
  expected: number;
  dropped_details: number;
  p50_us: number | null;
  p95_us: number | null;
  p99_us: number | null;
  max_us: number | null;
  bytes: number;
}

export interface SampleRow {
  worker: number;
  seq: number;
  start_unix_ms: number;
  end_unix_ms: number;
  latency_us: number;
  product_id: string;
  product_name?: string | null;
  request_index?: number | null;
  http_status?: number | null;
  success: boolean;
  expected: boolean;
  reason?: string | null;
  error_class?: string | null;
  request_id?: string | null;
  error?: string | null;
  response_bytes?: number | null;
}

export interface ReportPayload {
  id: string;
  template_id: string;
  target_id: string;
  mode: string;
  status: string;
  started_at: number | null;
  ended_at: number | null;
  processes: number;
  interrupt_ranges: { start: number; end: number }[] | null;
  error: string | null;
  counters: {
    samples: number;
    success: number;
    failure: number;
    expected: number;
    bytes: number;
    dropped_details: number;
  };
  percentiles: {
    p50_us: number | null;
    p95_us: number | null;
    p99_us: number | null;
    max_us: number | null;
  };
  windows: WindowRow[];
  timeline: TimelineEvent[];
  timeseries: TimePoint[];
  snapshot?: unknown;
  verdict?: Verdict | null;
}
