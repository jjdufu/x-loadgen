import type {
  AssertionsSpec,
  HttpAuthSpec,
  KeyValueRow,
  LoadConfig,
  MazbriRequestSpec,
  RequestBodySpec,
  RequestDraft,
  TenantSpec,
  TlsSpec,
} from "../api/types";

export const HTTP_METHODS = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
] as const;

export const BODY_CONTENT_TYPES = [
  { value: "", label: "无请求体" },
  { value: "application/json", label: "application/json" },
  {
    value: "application/x-www-form-urlencoded",
    label: "application/x-www-form-urlencoded",
  },
  { value: "multipart/form-data", label: "multipart/form-data" },
  { value: "text/plain", label: "text/plain" },
  { value: "application/xml", label: "application/xml" },
] as const;

export const MAZBRI_SUITES = [
  {
    value: "suite/aes-256-gcm",
    label: "suite/aes-256-gcm (AES-256-GCM 默认)",
  },
  {
    value: "suite/gm-sm4-hmac-sm3",
    label: "suite/gm-sm4-hmac-sm3 (国密 SM4-CBC + HMAC-SM3)",
  },
  {
    value: "suite/tenant-chacha20",
    label: "suite/tenant-chacha20 (ChaCha20-Poly1305)",
  },
] as const;

export const MAZBRI_DATA_ALGS = [
  { value: "aes-256-gcm", label: "aes-256-gcm (AEAD)" },
  { value: "aes-128-gcm", label: "aes-128-gcm (AEAD)" },
  { value: "aes-192-gcm", label: "aes-192-gcm (AEAD)" },
  { value: "chacha20-poly1305", label: "chacha20-poly1305 (AEAD)" },
  { value: "sm4-cbc-pkcs7", label: "sm4-cbc-pkcs7 (国密分组)" },
  { value: "sm4-ctr", label: "sm4-ctr (国密流式)" },
  { value: "aes-256-cbc", label: "aes-256-cbc" },
  { value: "aes-128-cbc", label: "aes-128-cbc" },
  { value: "aes-256-ctr", label: "aes-256-ctr" },
  { value: "aes-128-ctr", label: "aes-128-ctr" },
  { value: "aes-256-cfb", label: "aes-256-cfb" },
  { value: "aes-128-cfb", label: "aes-128-cfb" },
  { value: "aes-256-ecb", label: "aes-256-ecb (需开启兼容算法)" },
  { value: "aes-128-ecb", label: "aes-128-ecb (需开启兼容算法)" },
  { value: "sm4-ecb-pkcs7", label: "sm4-ecb-pkcs7 (需开启兼容算法)" },
  { value: "des-cbc-pkcs5", label: "des-cbc-pkcs5 (需开启兼容算法)" },
] as const;

export const MAZBRI_MAC_ALGS = [
  { value: "", label: "无外挂 MAC (仅 AEAD 可选)" },
  { value: "hmac-sha256", label: "hmac-sha256" },
  { value: "hmac-sha384", label: "hmac-sha384" },
  { value: "hmac-sha512", label: "hmac-sha512" },
  { value: "hmac-sm3", label: "hmac-sm3 (国密)" },
  { value: "hmac-md5", label: "hmac-md5 (需开启兼容算法)" },
] as const;

export function methodColorClass(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-emerald-600 dark:text-emerald-400";
    case "POST":
      return "text-amber-600 dark:text-amber-400";
    case "PUT":
      return "text-sky-600 dark:text-sky-400";
    case "PATCH":
      return "text-violet-600 dark:text-violet-400";
    case "DELETE":
      return "text-rose-600 dark:text-rose-400";
    case "HEAD":
      return "text-teal-600 dark:text-teal-400";
    case "OPTIONS":
      return "text-zinc-600 dark:text-zinc-400";
    default:
      return "text-foreground";
  }
}

export function methodBadgeClass(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
    case "POST":
      return "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-400";
    case "PUT":
      return "border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-400";
    case "PATCH":
      return "border-violet-500/25 bg-violet-500/10 text-violet-700 dark:text-violet-400";
    case "DELETE":
      return "border-rose-500/25 bg-rose-500/10 text-rose-700 dark:text-rose-400";
    default:
      return "border-border bg-muted text-foreground";
  }
}

export function statusCategoryClass(
  status: number | null | undefined,
  transportOk: boolean,
): string {
  if (!transportOk && !status) {
    return "border-rose-500/30 bg-rose-500/10 text-rose-700 dark:text-rose-400";
  }
  if (!status) {
    return "border-border bg-muted text-muted-foreground";
  }
  if (status >= 200 && status < 300) {
    return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
  }
  if (status >= 300 && status < 400) {
    return "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-400";
  }
  if (status >= 400 && status < 500) {
    return "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-400";
  }
  return "border-rose-500/30 bg-rose-500/10 text-rose-700 dark:text-rose-400";
}

export function formatLatencyUs(us: number | null | undefined): string {
  if (us === null || us === undefined) return "-";
  if (us < 1000) return `${us} µs`;
  const ms = us / 1000;
  if (ms < 1000) return `${ms.toFixed(ms < 10 ? 2 : 1)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "-";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function formatTimeMs(ms: number | null | undefined): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleString("zh-CN", { hour12: false });
}

export function formatRunStatus(status: string | null | undefined): string {
  switch (status) {
    case "running":
      return "运行中";
    case "stopping":
      return "停止中";
    case "finished":
      return "已完成";
    case "stopped":
      return "已停止";
    case "interrupted":
      return "已中断";
    case "failed":
      return "失败";
    default:
      return status || "-";
  }
}

export function formatRunMode(mode: string | null | undefined): string {
  switch (mode) {
    case "once":
      return "单轮";
    case "loop":
      return "循环";
    default:
      return mode || "-";
  }
}

export function formatVerdictResult(result: string | null | undefined): string {
  switch (result) {
    case "pass":
      return "通过";
    case "fail":
      return "未通过";
    case "pending":
      return "评估中";
    default:
      return "未配置";
  }
}

export function runStatusBadgeVariant(
  status: string | null | undefined,
): "success" | "warning" | "destructive" | "secondary" | "outline" {
  switch (status) {
    case "running":
      return "success";
    case "stopping":
    case "stopped":
      return "warning";
    case "failed":
    case "interrupted":
      return "destructive";
    case "finished":
      return "outline";
    default:
      return "secondary";
  }
}

export function formatTimelineEvent(
  event: string,
  rawPayload?: unknown,
): { label: string; detail: string } {
  const p: Record<string, unknown> =
    rawPayload !== null &&
    typeof rawPayload === "object" &&
    !Array.isArray(rawPayload)
      ? (rawPayload as Record<string, unknown>)
      : {};
  switch (event) {
    case "start": {
      const mode = typeof p.mode === "string" ? formatRunMode(p.mode) : "";
      return {
        label: "启动压测",
        detail: mode ? `执行模式：${mode}` : "",
      };
    }
    case "workers": {
      const count = p.value;
      return {
        label: "拉起工作进程",
        detail: count !== undefined ? `并发工作进程数：${String(count)}` : "",
      };
    }
    case "worker_exit": {
      const idx = p.value;
      return {
        label: "工作进程异常退出",
        detail: idx !== undefined ? `进程编号：w${String(idx)}` : "",
      };
    }
    case "traffic": {
      const fmtQps = (v: unknown) =>
        v === null || v === undefined ? "不限速" : `${String(v)} QPS`;
      const parts = [
        `速率 ${fmtQps(p.old_qps)} → ${fmtQps(p.qps)}`,
        `并发 ${String(p.old_concurrency ?? "-")} → ${String(p.concurrency ?? "-")}`,
      ];
      return {
        label: "动态调整流量",
        detail: parts.join("，"),
      };
    }
    case "window": {
      const idx = p.window_index !== undefined ? `#${String(p.window_index)}` : "-";
      const missed = Number(p.missed_sends ?? 0);
      return {
        label: "完成循环窗口",
        detail:
          missed > 0
            ? `轮次 ${idx}（过载漏发 ${missed} 次）`
            : `轮次 ${idx}`,
      };
    }
    case "stop": {
      const kindMap: Record<string, string> = {
        user: "手动停止",
        signal: "系统信号",
      };
      const kind = typeof p.kind === "string" ? (kindMap[p.kind] ?? p.kind) : "";
      return {
        label: "收到停止指令",
        detail: kind ? `触发方式：${kind}` : "",
      };
    }
    case "resume":
      return { label: "恢复续跑", detail: "" };
    case "interrupted": {
      const reason =
        p.reason === "process restarted"
          ? "服务进程重启，未完成任务转为中断状态"
          : typeof p.reason === "string"
            ? p.reason
            : "";
      return {
        label: "任务中断",
        detail: reason,
      };
    }
    case "end": {
      const st = typeof p.status === "string" ? formatRunStatus(p.status) : "-";
      const err = typeof p.error === "string" && p.error ? ` · 错误：${p.error}` : "";
      return {
        label: "任务结束",
        detail: `最终状态：${st}${err}`,
      };
    }
    default: {
      const hasValue =
        rawPayload !== undefined &&
        rawPayload !== null &&
        (typeof rawPayload !== "object" || Object.keys(p).length > 0);
      return {
        label: event,
        detail: hasValue ? JSON.stringify(rawPayload) : "",
      };
    }
  }
}

export function createEmptyKvRow(): KeyValueRow {
  return { key: "", value: "", active: true, description: "" };
}

export function createDefaultHttpAuth(authType = "inherit"): HttpAuthSpec {
  return {
    auth_type: authType,
    auth_active: true,
    token: "",
    username: "",
    password: "",
    key: "",
    value: "",
    add_to: "HEADERS",
  };
}

export function createDefaultTls(): TlsSpec {
  return {
    client_cert: "",
    client_key: "",
    ca_cert: "",
    insecure_skip_verify: false,
  };
}

export function createDefaultTenant(name = "default"): TenantSpec {
  return {
    name,
    weight: 1,
    api_key: "",
    encryption_mode: "none",
    tenant_id: "",
    tenant_key_base64: "",
    suite: "suite/aes-256-gcm",
    data_alg: "",
    mac_alg: "",
    require_signature: null,
    allow_insecure: false,
  };
}

export function createDefaultMazbriSpec(): MazbriRequestSpec {
  return {
    product_id: "",
    gateway_base_url: "",
    tenant_source: "environment",
    tenant_name: "",
    inline_tenant: createDefaultTenant("inline-tenant"),
  };
}

export function createDefaultBody(): RequestBodySpec {
  return {
    content_type: null,
    body: "",
    form: [],
  };
}

export function createDefaultAssertions(): AssertionsSpec {
  return {
    expect_transport: "ok",
    checks: [],
  };
}

export function createDefaultRequestDraft(
  protocol: "http" | "mazbri" = "http",
): RequestDraft {
  return {
    id: null,
    collection_id: null,
    name: "新建请求",
    protocol,
    method: protocol === "mazbri" ? "POST" : "GET",
    endpoint: protocol === "mazbri" ? "/v1/demo" : "",
    params: [],
    headers: [],
    body:
      protocol === "mazbri"
        ? {
            content_type: "application/json",
            body: '{\n  "ping": "mazbri"\n}',
            form: [],
          }
        : createDefaultBody(),
    auth: createDefaultHttpAuth("inherit"),
    mazbri: createDefaultMazbriSpec(),
    tls: createDefaultTls(),
    assertions: createDefaultAssertions(),
    captures: [],
    weight: 1,
    timeout_secs: null,
  };
}

export function createDefaultLoadConfig(): LoadConfig {
  return {
    model: "open_loop",
    qps: 20,
    concurrency: 10,
    requests: null,
    duration_secs: 10,
    warmup_secs: 0,
    ramp_secs: 0,
    timeout_secs: 30,
    connect_timeout_secs: 5,
    token_refresh_skew_secs: 60,
    include_request_id: true,
    trace_sampled: false,
    processes: 1,
    qps_mode: "total",
    requests_mode: "total",
    loop_enabled: false,
    rounds: null,
    loop_interval_secs: 0,
    expected_reasons: [],
    detail_retain_days: null,
    detail_retain_rows: null,
  };
}

export function normalizeTenantForApi(raw: TenantSpec): TenantSpec {
  const mode = (raw.encryption_mode || "none").trim();
  const isEncrypted = mode === "request" || mode === "full";
  const suite = isEncrypted ? (raw.suite?.trim() || null) : null;
  // When a named suite is chosen, omit explicit data_alg/mac_alg so they don't conflict
  const dataAlg = isEncrypted && !suite ? (raw.data_alg?.trim() || null) : null;
  const macAlg = isEncrypted && !suite ? (raw.mac_alg?.trim() || null) : null;

  return {
    id: raw.id,
    name: raw.name.trim() || "default",
    weight: Math.max(1, Number(raw.weight) || 1),
    api_key: raw.api_key?.trim() || null,
    encryption_mode: mode,
    tenant_id: isEncrypted ? (raw.tenant_id?.trim() || null) : null,
    tenant_key_base64: isEncrypted
      ? (raw.tenant_key_base64?.trim() || null)
      : null,
    suite,
    data_alg: dataAlg,
    mac_alg: macAlg,
    require_signature: isEncrypted ? (raw.require_signature ?? null) : null,
    allow_insecure: Boolean(raw.allow_insecure),
  };
}

export function normalizeDraftForApi(draft: RequestDraft): RequestDraft {
  const copy: RequestDraft = JSON.parse(JSON.stringify(draft)) as RequestDraft;
  copy.method = (copy.method || "GET").toUpperCase();
  copy.endpoint = copy.endpoint.trim();
  copy.weight = Math.max(1, Number(copy.weight) || 1);
  copy.params = (copy.params ?? []).filter(
    (r) => r.key.trim() !== "" || r.value.trim() !== "",
  );
  copy.headers = (copy.headers ?? []).filter(
    (r) => r.key.trim() !== "" || r.value.trim() !== "",
  );
  copy.body = {
    content_type: copy.body?.content_type?.trim() || null,
    body: copy.body?.body ?? null,
    form: (copy.body?.form ?? []).filter(
      (r) => r.key.trim() !== "" || r.value.trim() !== "",
    ),
  };
  copy.tls = {
    client_cert: copy.tls?.client_cert?.trim() || null,
    client_key: copy.tls?.client_key?.trim() || null,
    ca_cert: copy.tls?.ca_cert?.trim() || null,
    insecure_skip_verify: Boolean(copy.tls?.insecure_skip_verify),
  };
  copy.mazbri = {
    product_id: copy.mazbri?.product_id?.trim() ?? "",
    gateway_base_url: copy.mazbri?.gateway_base_url?.trim() || null,
    tenant_source: copy.mazbri?.tenant_source || "environment",
    tenant_name: copy.mazbri?.tenant_name?.trim() || null,
    inline_tenant:
      copy.mazbri?.tenant_source === "inline" && copy.mazbri?.inline_tenant
        ? normalizeTenantForApi(copy.mazbri.inline_tenant)
        : null,
  };
  copy.assertions = {
    expect_transport: copy.assertions?.expect_transport || "ok",
    checks: (copy.assertions?.checks ?? []).filter(
      (c) => c.path.trim() !== "",
    ),
  };
  copy.captures = (copy.captures ?? [])
    .map((c) => ({ name: c.name.trim(), path: c.path.trim() }))
    .filter((c) => c.name !== "" || c.path !== "");
  return copy;
}

export function cloneJson<T>(val: T): T {
  return JSON.parse(JSON.stringify(val)) as T;
}

/** How the target's response `traceparent` compared with the one x-loadgen sent. */
export type TraceEcho = "absent" | "match" | "mismatch";

export function traceEchoLabel(echo: TraceEcho | string | null | undefined): string {
  if (echo === "match") return "链路已接续";
  if (echo === "mismatch") return "链路未接续";
  return "未回 traceparent";
}

export function traceEchoVariant(
  echo: TraceEcho | string | null | undefined,
): "success" | "destructive" | "muted" {
  if (echo === "match") return "success";
  if (echo === "mismatch") return "destructive";
  return "muted";
}
