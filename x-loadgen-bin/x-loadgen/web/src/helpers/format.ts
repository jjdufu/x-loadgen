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
  { value: "", label: "None (无请求体)" },
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
    value: "suite/tenant-default-2026-11",
    label: "suite/tenant-default-2026-11 (AES-256-GCM 推荐默认)",
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
  { value: "aes-256-ecb", label: "aes-256-ecb (Legacy 需开启允许不安全)" },
  { value: "aes-128-ecb", label: "aes-128-ecb (Legacy 需开启允许不安全)" },
  { value: "sm4-ecb-pkcs7", label: "sm4-ecb-pkcs7 (Legacy 需开启允许不安全)" },
  { value: "des-cbc-pkcs5", label: "des-cbc-pkcs5 (Legacy 需开启允许不安全)" },
] as const;

export const MAZBRI_MAC_ALGS = [
  { value: "", label: "无外挂 MAC (仅 AEAD 可选)" },
  { value: "hmac-sha256", label: "hmac-sha256" },
  { value: "hmac-sha384", label: "hmac-sha384" },
  { value: "hmac-sha512", label: "hmac-sha512" },
  { value: "hmac-sm3", label: "hmac-sm3 (国密)" },
  { value: "hmac-md5", label: "hmac-md5 (Legacy 需开启允许不安全)" },
] as const;

export const MAZBRI_BINDING_ALGS = [
  { value: "", label: "不启用请求绑定签名 (x-mazbri-*)" },
  { value: "hmac-sha256", label: "hmac-sha256 (对称 Base64)", asymmetric: false },
  { value: "hmac-sha384", label: "hmac-sha384 (对称 Base64)", asymmetric: false },
  { value: "hmac-sha512", label: "hmac-sha512 (对称 Base64)", asymmetric: false },
  { value: "hmac-sm3", label: "hmac-sm3 (国密对称 Base64)", asymmetric: false },
  { value: "ed25519", label: "ed25519 (非对称密钥对)", asymmetric: true },
  { value: "sm2-sign", label: "sm2-sign (国密 SM2 非对称)", asymmetric: true },
  {
    value: "ecdsa-p256-sha256",
    label: "ecdsa-p256-sha256 (ECDSA P-256)",
    asymmetric: true,
  },
  {
    value: "ecdsa-p384-sha384",
    label: "ecdsa-p384-sha384 (ECDSA P-384)",
    asymmetric: true,
  },
  {
    value: "rsa-pkcs1v15-sign-sha256",
    label: "rsa-pkcs1v15-sign-sha256 (RSA-2048)",
    asymmetric: true,
  },
  {
    value: "rsa-pkcs1v15-sign-sha384",
    label: "rsa-pkcs1v15-sign-sha384 (RSA-2048)",
    asymmetric: true,
  },
  {
    value: "rsa-pkcs1v15-sign-sha512",
    label: "rsa-pkcs1v15-sign-sha512 (RSA-2048)",
    asymmetric: true,
  },
] as const;

export function isAsymmetricBindingAlg(alg: string | null | undefined): boolean {
  if (!alg) return false;
  const found = MAZBRI_BINDING_ALGS.find((item) => item.value === alg);
  return Boolean(found && "asymmetric" in found && found.asymmetric);
}

export function methodColorClass(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-emerald-500";
    case "POST":
      return "text-amber-500";
    case "PUT":
      return "text-sky-500";
    case "PATCH":
      return "text-violet-500";
    case "DELETE":
      return "text-rose-500";
    case "HEAD":
      return "text-teal-500";
    case "OPTIONS":
      return "text-pink-500";
    default:
      return "text-secondaryDark";
  }
}

export function statusCategoryClass(
  status: number | null | undefined,
  transportOk: boolean,
): string {
  if (!transportOk && !status) return "critical-error-response";
  if (!status) return "missing-data-response";
  if (status >= 100 && status < 200) return "info-response";
  if (status >= 200 && status < 300) return "success-response";
  if (status >= 300 && status < 400) return "redirect-response";
  if (status >= 400 && status < 500) return "critical-error-response";
  return "server-error-response";
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
    key_version: "v1",
    suite: "suite/tenant-default-2026-11",
    data_alg: "",
    mac_alg: "",
    require_signature: null,
    allow_insecure: false,
    binding_kid: "",
    binding_alg: "",
    binding_key: "",
    binding_public_key: "",
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
    name: protocol === "mazbri" ? "新建 Mazbri 请求" : "新建 HTTP 请求",
    protocol,
    method: protocol === "mazbri" ? "POST" : "GET",
    endpoint:
      protocol === "mazbri"
        ? "/v1/demo"
        : "https://echo.hoppscotch.io",
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
  const bindingAlg = raw.binding_alg?.trim() || null;

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
    key_version: isEncrypted ? (raw.key_version?.trim() || null) : null,
    suite,
    data_alg: dataAlg,
    mac_alg: macAlg,
    require_signature: isEncrypted ? (raw.require_signature ?? null) : null,
    allow_insecure: Boolean(raw.allow_insecure),
    binding_kid: bindingAlg ? (raw.binding_kid?.trim() || null) : null,
    binding_alg: bindingAlg,
    binding_key: bindingAlg ? (raw.binding_key?.trim() || null) : null,
    binding_public_key: bindingAlg
      ? (raw.binding_public_key?.trim() || null)
      : null,
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
  return copy;
}

export function cloneJson<T>(val: T): T {
  return JSON.parse(JSON.stringify(val)) as T;
}
