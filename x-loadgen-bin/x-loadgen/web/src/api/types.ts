export type RequestProtocol = "http" | "mazbri";

export type HttpMethod =
  | "GET"
  | "POST"
  | "PUT"
  | "PATCH"
  | "DELETE"
  | "HEAD"
  | "OPTIONS";

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

export interface KeyValueRow {
  key: string;
  value: string;
  active: boolean;
  description?: string | null;
}

export interface EnvVarRow {
  key: string;
  value: string;
  secret: boolean;
  active: boolean;
}

export interface RequestBodySpec {
  content_type?: string | null;
  body?: string | null;
  form: KeyValueRow[];
}

export type HttpAuthType = "inherit" | "none" | "bearer" | "basic" | "api-key";

export interface HttpAuthSpec {
  auth_type: HttpAuthType | string;
  auth_active: boolean;
  token?: string | null;
  username?: string | null;
  password?: string | null;
  key?: string | null;
  value?: string | null;
  add_to?: "HEADERS" | "QUERY_PARAMS" | string | null;
}

export interface TlsSpec {
  client_cert?: string | null;
  client_key?: string | null;
  ca_cert?: string | null;
  insecure_skip_verify: boolean;
}

export interface FieldCheck {
  path: string;
  eq?: unknown;
}

export interface AssertionsSpec {
  expect_transport: "ok" | "reject" | string;
  checks: FieldCheck[];
}

export type EncryptionMode = "none" | "request" | "full";

export interface TenantSpec {
  id?: string;
  name: string;
  weight: number;
  api_key?: string | null;
  encryption_mode: EncryptionMode | string;
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

export interface MazbriRequestSpec {
  product_id: string;
  gateway_base_url?: string | null;
  tenant_source: "environment" | "inline" | string;
  tenant_name?: string | null;
  inline_tenant?: TenantSpec | null;
}

export interface GeneratedBindingKeypair {
  binding_kid: string;
  binding_alg: string;
  private_key_pem?: string | null;
  public_key_pem: string;
}

export interface RequestDraft {
  id?: string | null;
  collection_id?: string | null;
  name: string;
  sort_order?: number | null;
  protocol: RequestProtocol | string;
  method: string;
  endpoint: string;
  params: KeyValueRow[];
  headers: KeyValueRow[];
  body: RequestBodySpec;
  auth: HttpAuthSpec;
  mazbri: MazbriRequestSpec;
  tls: TlsSpec;
  assertions: AssertionsSpec;
  weight: number;
  timeout_secs?: number | null;
}

export interface SavedRequest extends RequestDraft {
  id: string;
  collection_id: string;
  sort_order: number;
  created_at: number;
  updated_at: number;
}

export interface CollectionNode {
  id: string;
  parent_id?: string | null;
  name: string;
  sort_order: number;
  default_protocol: RequestProtocol | string;
  auth: HttpAuthSpec;
  headers: KeyValueRow[];
  variables: EnvVarRow[];
  description?: string | null;
  folders: CollectionNode[];
  requests: SavedRequest[];
  created_at: number;
  updated_at: number;
}

export interface CollectionInput {
  parent_id?: string | null;
  name: string;
  sort_order?: number | null;
  default_protocol?: RequestProtocol | string;
  auth?: HttpAuthSpec;
  headers?: KeyValueRow[];
  variables?: EnvVarRow[];
  description?: string | null;
}

export interface EnvironmentSummary {
  id: string;
  name: string;
  is_global: boolean;
  gateway_base_url?: string | null;
  variable_count: number;
  tenant_count: number;
  note?: string | null;
  created_at: number;
  updated_at: number;
}

export interface EnvironmentDetail {
  id: string;
  name: string;
  is_global: boolean;
  gateway_base_url?: string | null;
  variables: EnvVarRow[];
  http_auth: HttpAuthSpec;
  tls: TlsSpec;
  note?: string | null;
  tenants: TenantSpec[];
  created_at: number;
  updated_at: number;
}

export interface EnvironmentInput {
  name: string;
  gateway_base_url?: string | null;
  variables: EnvVarRow[];
  http_auth?: HttpAuthSpec;
  tls?: TlsSpec;
  note?: string | null;
  tenants?: TenantSpec[];
}

export interface DetailedCheckResult {
  path: string;
  expected?: unknown;
  actual?: unknown;
  passed: boolean;
  message?: string | null;
}

export interface ProbeRequestPreview {
  method: string;
  url: string;
  headers: [string, string][];
  body_preview?: string | null;
  unresolved_variables: string[];
}

export interface MazbriWireDebug {
  tenant_name: string;
  encryption_mode: string;
  suite?: string | null;
  data_alg?: string | null;
  mac_alg?: string | null;
  binding_alg?: string | null;
  binding_kid?: string | null;
  token_url: string;
  token_exchange_us: number;
  token_preview?: string | null;
  token_expires_in?: number | null;
  invoke_url: string;
  binding_headers: Record<string, string>;
  inner_gateway_request: unknown;
  wire_request_body: string;
  wire_response_status?: number | null;
  wire_response_body?: string | null;
  decoded_envelope?: unknown;
}

export interface ProbeRequest {
  request_id?: string | null;
  request?: RequestDraft | null;
  collection_id?: string | null;
  environment_id?: string | null;
  variables_override?: Record<string, string> | null;
  seq?: number | null;
}

export interface ProbeResponse {
  protocol: RequestProtocol | string;
  transport_ok: boolean;
  passed_assertions: boolean;
  http_status?: number | null;
  status_text?: string | null;
  latency_us: number;
  response_bytes: number;
  headers: [string, string][];
  body_text: string;
  body_json?: unknown;
  error_class?: string | null;
  error?: string | null;
  assertions: DetailedCheckResult[];
  request_preview: ProbeRequestPreview;
  mazbri_wire?: MazbriWireDebug | null;
}

export interface AssertSpec {
  p50_ms?: number | null;
  p95_ms?: number | null;
  p99_ms?: number | null;
  max_ms?: number | null;
  max_error_rate?: number | null;
  min_qps_ratio?: number | null;
}

export interface LoadConfig {
  model: "open_loop" | "closed_loop" | string;
  qps?: number | null;
  concurrency: number;
  requests?: number | null;
  duration_secs?: number | null;
  warmup_secs: number;
  ramp_secs: number;
  timeout_secs: number;
  connect_timeout_secs: number;
  token_refresh_skew_secs: number;
  include_request_id: boolean;
  processes: number;
  qps_mode: "total" | "per_process" | string;
  requests_mode: "total" | "per_process" | string;
  loop_enabled: boolean;
  rounds?: number | null;
  loop_interval_secs: number;
  expected_reasons: string[];
  detail_retain_days?: number | null;
  detail_retain_rows?: number | null;
}

export interface LoadProfile extends LoadConfig {
  id: string;
  name: string;
  assert?: AssertSpec | null;
  note?: string | null;
  created_at: number;
  updated_at: number;
}

export interface LoadProfileInput extends LoadConfig {
  name: string;
  assert?: AssertSpec | null;
  note?: string | null;
}

export interface SelectedRequestWeight {
  request_id: string;
  weight: number;
}

export interface CreateRunRequest {
  request_id?: string | null;
  collection_id?: string | null;
  selected_requests?: SelectedRequestWeight[] | null;
  adhoc_request?: RequestDraft | null;
  adhoc_requests?: RequestDraft[] | null;
  environment_id?: string | null;
  variables_override?: Record<string, string> | null;
  load_profile_id?: string | null;
  load_profile?: string | null;
  mode?: "once" | "loop" | string;
  model?: "open_loop" | "closed_loop" | string;
  qps?: number | null;
  concurrency?: number | null;
  requests?: number | null;
  duration_secs?: number | null;
  warmup_secs?: number | null;
  ramp_secs?: number | null;
  timeout_secs?: number | null;
  connect_timeout_secs?: number | null;
  token_refresh_skew_secs?: number | null;
  include_request_id?: boolean | null;
  processes?: number | null;
  qps_mode?: "total" | "per_process" | string | null;
  requests_mode?: "total" | "per_process" | string | null;
  loop_enabled?: boolean | null;
  rounds?: number | null;
  clear_rounds?: boolean | null;
  loop_interval_secs?: number | null;
  expected_reasons?: string[] | null;
  detail_retain_days?: number | null;
  detail_retain_rows?: number | null;
  assert?: AssertSpec | null;
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
  title?: string | null;
  source?: Record<string, unknown> | null;
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
