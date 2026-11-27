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

/** Stores the value at `path` (same syntax as assertion checks) as `{{captures.<name>}}`. */
export interface CaptureSpec {
  name: string;
  path: string;
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
  captures: CaptureSpec[];
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
  /** `traceparent` sampled flag; probes default to sampled (`01`). */
  trace_sampled?: boolean;
}

export interface ProbeResponse {
  /** Id of this probe in the probe history (absent when history is off). */
  history_id?: number | null;
  protocol: RequestProtocol | string;
  tenant_name?: string | null;
  /** `X-Request-Id` sent (`probe-<UUIDv7>`). */
  request_id: string;
  trace_id: string;
  traceparent: string;
  response_traceparent?: string | null;
  trace_echo: "absent" | "match" | "mismatch" | string;
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
  /** Values this call captured (`{{captures.<name>}}` in later requests). */
  captures: Record<string, string>;
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
  trace_sampled: boolean;
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
  title?: string | null;
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
  trace_sampled?: boolean | null;
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
  model?: string | null;
  status: string;
  started_at: number | null;
  ended_at: number | null;
  current_qps: number | null;
  current_concurrency: number | null;
  processes: number;
  interrupt_ranges: { start: number; end: number }[] | null;
  error: string | null;
  created_at: number;
  protocols?: string[];
  environment_name?: string | null;
  samples?: number | null;
  success?: number | null;
  failure?: number | null;
  p95_us?: number | null;
  p99_us?: number | null;
  achieved_qps?: number | null;
  verdict_result?: "pass" | "fail" | "pending" | null;
  snapshot?: unknown;
  verdict?: Verdict | null;
}

export interface RunReplayPayload {
  run_id: string;
  title: string;
  created_at: number;
  mode: string;
  source: {
    kind: string;
    request_id?: string | null;
    collection_id?: string | null;
    environment_id?: string | null;
    environment_name?: string | null;
    load_profile_id?: string | null;
    load_profile_name?: string | null;
  };
  environment_id?: string | null;
  environment_name?: string | null;
  load_config: LoadConfig;
  assert?: AssertSpec | null;
  requests: RequestDraft[];
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
  missed_sends?: number;
  min_us?: number | null;
  avg_us?: number | null;
  stdev_us?: number | null;
  p50_us: number | null;
  p75_us?: number | null;
  p90_us?: number | null;
  p95_us: number | null;
  p99_us: number | null;
  p999_us?: number | null;
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
  request_bytes?: number;
  warmup?: boolean;
  in_flight_max?: number;
  avg_latency_us?: number | null;
  p50_us?: number | null;
  p90_us?: number | null;
  p95_us?: number | null;
  p99_us?: number | null;
  max_us?: number | null;
  error_rate?: number;
  error_classes?: Record<string, number> | null;
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
  protocol?: string;
  http_method?: string | null;
  product_id: string;
  product_name?: string | null;
  customer_name?: string | null;
  request_index?: number | null;
  http_status?: number | null;
  success: boolean;
  expected: boolean;
  reason?: string | null;
  error_class?: string | null;
  request_id?: string | null;
  error?: string | null;
  request_bytes?: number | null;
  response_bytes?: number | null;
  queue_us?: number | null;
  ttfb_us?: number | null;
  transfer_us?: number | null;
  crypto_seal_us?: number | null;
  crypto_unseal_us?: number | null;
  token_wait_us?: number | null;
  /** W3C trace id sent in `traceparent` (32 lowercase hex). */
  trace_id?: string | null;
  /** `match` / `mismatch` / `absent`: response traceparent vs the sent one. */
  trace_echo?: string;
  warmup?: boolean;
  /** Values extracted by the request's `captures` (secrets scrubbed). */
  captures?: Record<string, string>;
  /** Failed samples only: response body cut to 2 KB (secrets scrubbed). */
  response_body?: string | null;
}

/** Server-side filters for `GET /runs/:id/samples`. */
export interface SampleFilters {
  success?: boolean;
  product_id?: string;
  error_class?: string;
  min_latency_us?: number;
  tenant?: string;
  request_id?: string;
  trace_id?: string;
}

/** One product / tenant row of the report breakdown. */
export interface BreakdownGroup {
  product_id?: string;
  tenant?: string;
  protocol?: string;
  method?: string | null;
  samples: number;
  share_pct?: number;
  qps?: number | null;
  success: number;
  failure: number;
  expected: number;
  error_rate?: number;
  min_us?: number | null;
  avg_us?: number | null;
  stdev_us?: number | null;
  p50_us: number | null;
  p75_us?: number | null;
  p90_us?: number | null;
  p95_us: number | null;
  p99_us: number | null;
  p999_us?: number | null;
  max_us: number | null;
  total_bytes?: number;
  avg_bytes?: number | null;
}

export interface BreakdownError {
  error_class: string;
  error: string;
  count: number;
  share_pct?: number;
  http_status: Record<string, number>;
}

export interface HistogramBin {
  label: string;
  from_ms: number;
  to_ms: number | null;
  count: number;
  pct: number;
  cumulative_pct: number;
}

export interface ApdexStats {
  score: number | null;
  satisfied_ms: number;
  tolerated_ms: number;
  satisfied: number;
  tolerated: number;
  frustrated: number;
  failed: number;
  expected: number;
}

export interface PhaseTimingStats {
  samples: number;
  avg_us: number | null;
  p50_us: number | null;
  p95_us: number | null;
  p99_us: number | null;
  max_us: number | null;
  share_pct: number | null;
}

export interface HttpProtocolBreakdown {
  samples: number;
  methods: Record<string, number>;
  status_families: Record<string, number>;
  phases: {
    queue: PhaseTimingStats;
    ttfb: PhaseTimingStats;
    transfer: PhaseTimingStats;
    total: PhaseTimingStats;
  };
  request_sizes: {
    total_bytes: number;
    avg_bytes: number | null;
  };
  response_sizes: {
    total_bytes: number;
    min_bytes: number | null;
    avg_bytes: number | null;
    p95_bytes: number | null;
    max_bytes: number | null;
    buckets: { label: string; count: number; pct: number }[];
  };
}

export interface MazbriProtocolBreakdown {
  samples: number;
  methods: Record<string, number>;
  encryption_modes: string[];
  suites: string[];
  crypto_overhead_pct: number | null;
  wire_pct: number | null;
  trace_echo: { match: number; mismatch: number; absent: number };
  phases: {
    token_wait: PhaseTimingStats;
    crypto_seal: PhaseTimingStats;
    gateway_ttfb: PhaseTimingStats;
    transfer: PhaseTimingStats;
    crypto_unseal: PhaseTimingStats;
    queue: PhaseTimingStats;
    end_to_end: PhaseTimingStats;
  };
}

/** `report.breakdown`: computed from recorded (non-warmup) samples. */
export interface ReportBreakdown {
  basis: string;
  samples: number;
  products: BreakdownGroup[];
  tenants: BreakdownGroup[];
  status_codes: Record<string, number>;
  status_families?: Record<string, number>;
  top_errors: BreakdownError[];
  latency_histogram?: HistogramBin[];
  apdex?: ApdexStats;
  protocol_breakdown?: {
    http?: HttpProtocolBreakdown;
    mazbri?: MazbriProtocolBreakdown;
  };
}

export interface ReportSummary {
  effective_elapsed_ms: number | null;
  wall_elapsed_ms: number | null;
  achieved_qps: number | null;
  peak_qps: number | null;
  configured_qps: number | null;
  qps_ratio: number | null;
  success_rate: number | null;
  error_rate: number | null;
  expected_rate: number | null;
  total_request_bytes: number;
  total_response_bytes: number;
  avg_response_bytes: number | null;
  avg_bandwidth_bps: number | null;
  avg_request_bandwidth_bps: number | null;
  peak_in_flight: number;
  apdex: ApdexStats;
  protocols: string[];
  shape: "single_http" | "multi_http" | "single_mazbri" | "multi_mazbri" | "mixed" | string;
  has_http: boolean;
  has_mazbri: boolean;
  has_multiple_endpoints: boolean;
  has_multiple_tenants: boolean;
  has_multiple_windows: boolean;
  data_integrity: {
    dropped_details: number;
    missed_sends: number;
    recorded_samples: number;
    total_samples: number;
  };
}

/** Keyset cursor returned as `next`; send all three fields back verbatim. */
export interface SampleCursor {
  after_end_unix_ms: number;
  after_worker: number;
  after_seq: number;
}

export interface ReportPayload {
  id: string;
  title?: string | null;
  source?: Record<string, unknown> | null;
  template_id: string;
  target_id: string;
  mode: string;
  status: string;
  started_at: number | null;
  ended_at: number | null;
  current_qps?: number | null;
  current_concurrency?: number | null;
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
    missed_sends?: number;
    peak_in_flight?: number;
  };
  percentiles: {
    min_us?: number | null;
    avg_us?: number | null;
    stdev_us?: number | null;
    p50_us: number | null;
    p75_us?: number | null;
    p90_us?: number | null;
    p95_us: number | null;
    p99_us: number | null;
    p999_us?: number | null;
    max_us: number | null;
  };
  summary?: ReportSummary;
  windows: WindowRow[];
  timeline: TimelineEvent[];
  timeseries: TimePoint[];
  snapshot?: unknown;
  verdict?: Verdict | null;
  breakdown?: ReportBreakdown;
}

export interface CompareMetricDiff {
  key: string;
  label: string;
  unit: string;
  base: number | null;
  target: number | null;
  delta: number | null;
  delta_pct: number | null;
  higher_is_better: boolean | null;
  status: "improved" | "regressed" | "neutral";
}

export interface CompareProductDiff {
  product_id: string;
  base_samples: number | null;
  target_samples: number | null;
  base_qps: number | null;
  target_qps: number | null;
  base_p50_us: number | null;
  target_p50_us: number | null;
  base_p95_us: number | null;
  target_p95_us: number | null;
  p95_delta_pct: number | null;
  base_p99_us: number | null;
  target_p99_us: number | null;
  base_error_rate: number | null;
  target_error_rate: number | null;
}

export interface CompareAlignedPoint {
  offset_sec: number;
  base_qps: number | null;
  target_qps: number | null;
  base_p95_us: number | null;
  target_p95_us: number | null;
  base_p99_us: number | null;
  target_p99_us: number | null;
  base_error_rate: number | null;
  target_error_rate: number | null;
}

export interface CompareReportPayload {
  base: ReportPayload;
  target: ReportPayload;
  config_diff: {
    base: Record<string, unknown>;
    target: Record<string, unknown>;
  };
  metrics: CompareMetricDiff[];
  products_diff: CompareProductDiff[];
  aligned_series: CompareAlignedPoint[];
}

/** One entry of the probe history (`GET /api/v1/probes`). */
export interface ProbeRecord {
  id: number;
  kind: "probe";
  created_at: number;
  saved_request_id?: string | null;
  request_name: string;
  protocol: RequestProtocol | string;
  method: string;
  url: string;
  environment_id?: string | null;
  environment_name?: string | null;
  tenant_name?: string | null;
  product_id?: string | null;
  request_id: string;
  trace_id: string;
  traceparent: string;
  trace_echo: "absent" | "match" | "mismatch";
  http_status?: number | null;
  transport_ok: boolean;
  passed_assertions: boolean;
  latency_us: number;
  request_bytes?: number;
  response_bytes: number;
  error_class?: string | null;
  error?: string | null;
  captures: Record<string, string>;
  /** Complete draft including credentials; returned by `GET /api/v1/probes/{id}`. */
  request_draft?: RequestDraft | null;
  variables_override?: Record<string, string> | null;
  response_headers?: [string, string][] | null;
  /** Only returned by `GET /api/v1/probes/{id}`. */
  response_body?: string;
  response_truncated: boolean;
}

export interface ProbesPage {
  items: ProbeRecord[];
  next_before_id: number | null;
}

export interface ProbesQuery {
  limit?: number;
  before_id?: number;
  request_id?: string;
  trace_id?: string;
  saved_request_id?: string;
  protocol?: string;
  q?: string;
}
