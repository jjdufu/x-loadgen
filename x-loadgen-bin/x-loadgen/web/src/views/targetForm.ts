// Target form model: page fields <-> API payload for /api/v1/targets.
import type { Product, TargetDetail, TargetProtocol, Tenant } from "../api/types";

export interface Option {
  value: string;
  label: string;
}

export const SUITES: Option[] = [
  { value: "suite/tenant-default-2026-11", label: "tenant-default-2026-11（AES-256-GCM）" },
  { value: "suite/gm-sm4-hmac-sm3", label: "gm-sm4-hmac-sm3（SM4-CBC + HMAC-SM3）" },
  { value: "suite/tenant-chacha20", label: "tenant-chacha20（ChaCha20-Poly1305）" },
];

export const DATA_ALGS: Option[] = [
  { value: "aes-256-gcm", label: "aes-256-gcm（AEAD）" },
  { value: "aes-192-gcm", label: "aes-192-gcm（AEAD）" },
  { value: "aes-128-gcm", label: "aes-128-gcm（AEAD）" },
  { value: "chacha20-poly1305", label: "chacha20-poly1305（AEAD）" },
  { value: "sm4-cbc-pkcs7", label: "sm4-cbc-pkcs7（需 MAC）" },
  { value: "sm4-ctr", label: "sm4-ctr（需 MAC）" },
  { value: "aes-256-cbc", label: "aes-256-cbc（需 MAC）" },
  { value: "aes-128-cbc", label: "aes-128-cbc（需 MAC）" },
  { value: "aes-256-ctr", label: "aes-256-ctr（需 MAC）" },
  { value: "aes-128-ctr", label: "aes-128-ctr（需 MAC）" },
  { value: "aes-256-cfb", label: "aes-256-cfb（需 MAC）" },
  { value: "aes-128-cfb", label: "aes-128-cfb（需 MAC）" },
  { value: "aes-256-ecb", label: "aes-256-ecb（legacy，需允许不安全算法）" },
  { value: "aes-128-ecb", label: "aes-128-ecb（legacy，需允许不安全算法）" },
  { value: "sm4-ecb-pkcs7", label: "sm4-ecb-pkcs7（legacy，需允许不安全算法）" },
  { value: "des-cbc-pkcs5", label: "des-cbc-pkcs5（legacy，需允许不安全算法）" },
];

export const MAC_ALGS: Option[] = [
  { value: "", label: "不使用（仅 AEAD）" },
  { value: "hmac-sha256", label: "hmac-sha256" },
  { value: "hmac-sha384", label: "hmac-sha384" },
  { value: "hmac-sha512", label: "hmac-sha512" },
  { value: "hmac-sm3", label: "hmac-sm3" },
  { value: "hmac-md5", label: "hmac-md5（legacy，需允许不安全算法）" },
];

export const BINDING_ALGS: Option[] = [
  { value: "", label: "不启用请求绑定" },
  { value: "hmac-sha256", label: "hmac-sha256（对称，mgmt 签发的 Base64 密钥）" },
  { value: "hmac-sha384", label: "hmac-sha384（对称）" },
  { value: "hmac-sha512", label: "hmac-sha512（对称）" },
  { value: "hmac-sm3", label: "hmac-sm3（对称）" },
  { value: "rsa-pkcs1v15-sign-sha256", label: "rsa-pkcs1v15-sign-sha256" },
  { value: "rsa-pkcs1v15-sign-sha384", label: "rsa-pkcs1v15-sign-sha384" },
  { value: "rsa-pkcs1v15-sign-sha512", label: "rsa-pkcs1v15-sign-sha512" },
  { value: "ecdsa-p256-sha256", label: "ecdsa-p256-sha256" },
  { value: "ecdsa-p384-sha384", label: "ecdsa-p384-sha384" },
  { value: "ed25519", label: "ed25519" },
  { value: "sm2-sign", label: "sm2-sign" },
];

export const LEGACY_ALGS = new Set([
  "aes-256-ecb",
  "aes-128-ecb",
  "sm4-ecb-pkcs7",
  "des-cbc-pkcs5",
  "hmac-md5",
]);

export function isHmacBinding(alg: string): boolean {
  return alg.startsWith("hmac-");
}

export type AlgSource = "suite" | "custom";
export type SignatureChoice = "" | "true" | "false";

export interface FormTenant {
  id: string;
  name: string;
  weight: number;
  api_key: string;
  encryption_mode: string;
  alg_source: AlgSource;
  suite: string;
  data_alg: string;
  mac_alg: string;
  require_signature: SignatureChoice;
  allow_insecure: boolean;
  tenant_id: string;
  key_version: string;
  tenant_key_base64: string;
  binding_alg: string;
  binding_kid: string;
  binding_key: string;
  binding_public_key: string;
}

export interface FormProduct {
  name: string;
  product_id: string;
  path: string;
  method: string;
  weight: number;
  qps_limit: number | null;
  timeout_secs: number | null;
  query_text: string;
  headers_text: string;
  body_text: string;
  expect_transport: string;
  checks_text: string;
}

export interface TargetForm {
  name: string;
  protocol: TargetProtocol;
  gateway_base_url: string;
  insecure_skip_verify: boolean;
  note: string;
  client_cert: string;
  client_key: string;
  ca_cert: string;
  tenants: FormTenant[];
  products: FormProduct[];
}

export function emptyTenant(): FormTenant {
  return {
    id: "",
    name: "",
    weight: 1,
    api_key: "",
    encryption_mode: "none",
    alg_source: "suite",
    suite: SUITES[0].value,
    data_alg: "aes-256-gcm",
    mac_alg: "",
    require_signature: "",
    allow_insecure: false,
    tenant_id: "",
    key_version: "",
    tenant_key_base64: "",
    binding_alg: "",
    binding_kid: "",
    binding_key: "",
    binding_public_key: "",
  };
}

export function emptyProduct(): FormProduct {
  return {
    name: "",
    product_id: "",
    path: "/",
    method: "POST",
    weight: 1,
    qps_limit: null,
    timeout_secs: null,
    query_text: "",
    headers_text: "",
    body_text: "",
    expect_transport: "ok",
    checks_text: "",
  };
}

export function emptyForm(): TargetForm {
  return {
    name: "",
    protocol: "mazbri",
    gateway_base_url: "",
    insecure_skip_verify: false,
    note: "",
    client_cert: "",
    client_key: "",
    ca_cert: "",
    tenants: [emptyTenant()],
    products: [emptyProduct()],
  };
}

function jsonText(value: unknown): string {
  if (value === null || value === undefined) return "";
  return JSON.stringify(value, null, 2);
}

export function tenantFromApi(tenant: Tenant): FormTenant {
  const base = emptyTenant();
  const custom = !tenant.suite && !!tenant.data_alg;
  return {
    ...base,
    id: tenant.id ?? "",
    name: tenant.name,
    weight: tenant.weight,
    api_key: tenant.api_key ?? "",
    encryption_mode: tenant.encryption_mode,
    alg_source: custom ? "custom" : "suite",
    suite: tenant.suite ?? base.suite,
    data_alg: tenant.data_alg ?? base.data_alg,
    mac_alg: tenant.mac_alg ?? "",
    require_signature:
      tenant.require_signature === null || tenant.require_signature === undefined
        ? ""
        : tenant.require_signature
          ? "true"
          : "false",
    allow_insecure: tenant.allow_insecure ?? false,
    tenant_id: tenant.tenant_id ?? "",
    key_version: tenant.key_version ?? "",
    tenant_key_base64: tenant.tenant_key_base64 ?? "",
    binding_alg: tenant.binding_alg ?? "",
    binding_kid: tenant.binding_kid ?? "",
    binding_key: tenant.binding_key ?? "",
    binding_public_key: tenant.binding_public_key ?? "",
  };
}

export function formFromDetail(detail: TargetDetail): TargetForm {
  const tenants = (detail.tenants ?? []).map(tenantFromApi);
  const products = (detail.products ?? []).map(
    (product: Product): FormProduct => ({
      name: product.name,
      product_id: product.product_id,
      path: product.path,
      method: product.method,
      weight: product.weight,
      qps_limit: product.qps_limit ?? null,
      timeout_secs: product.timeout_secs ?? null,
      query_text: jsonText(product.query_json),
      headers_text: jsonText(product.headers_json),
      body_text: jsonText(product.body_json),
      expect_transport: product.expect_transport ?? "ok",
      checks_text: jsonText(product.expect_checks_json),
    }),
  );
  return {
    name: detail.name,
    protocol: detail.protocol ?? "http",
    gateway_base_url: detail.gateway_base_url,
    insecure_skip_verify: detail.insecure_skip_verify,
    note: detail.note ?? "",
    client_cert: detail.client_cert ?? "",
    client_key: detail.client_key ?? "",
    ca_cert: detail.ca_cert ?? "",
    tenants: tenants.length ? tenants : [emptyTenant()],
    products: products.length ? products : [emptyProduct()],
  };
}

function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

/** Parse an optional JSON editor field; throws with the field label on bad JSON. */
function parseJson(text: string, label: string): unknown {
  if (!text.trim()) return null;
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`${label} 不是合法的 JSON`);
  }
}

export function tenantPayload(tenant: FormTenant, protocol: TargetProtocol): Record<string, unknown> {
  const common = { name: tenant.name, weight: tenant.weight, api_key: tenant.api_key.trim() };
  if (protocol === "http") {
    return { ...common, encryption_mode: "none" };
  }
  const encrypted = tenant.encryption_mode !== "none";
  const custom = tenant.alg_source === "custom";
  const binding = tenant.binding_alg !== "";
  return {
    ...common,
    encryption_mode: tenant.encryption_mode,
    tenant_id: orNull(tenant.tenant_id),
    key_version: orNull(tenant.key_version),
    tenant_key_base64: encrypted ? orNull(tenant.tenant_key_base64) : null,
    suite: encrypted && !custom ? orNull(tenant.suite) : null,
    data_alg: encrypted && custom ? orNull(tenant.data_alg) : null,
    mac_alg: encrypted && custom ? orNull(tenant.mac_alg) : null,
    require_signature: encrypted && tenant.require_signature !== "" ? tenant.require_signature === "true" : null,
    allow_insecure: encrypted && tenant.allow_insecure,
    binding_kid: binding ? orNull(tenant.binding_kid) : null,
    binding_alg: binding ? tenant.binding_alg : null,
    binding_key: binding ? orNull(tenant.binding_key) : null,
  };
}

export function targetPayload(form: TargetForm): Record<string, unknown> {
  return {
    name: form.name,
    protocol: form.protocol,
    gateway_base_url: form.gateway_base_url,
    insecure_skip_verify: form.insecure_skip_verify,
    note: orNull(form.note),
    client_cert: orNull(form.client_cert),
    client_key: orNull(form.client_key),
    ca_cert: orNull(form.ca_cert),
    tenants: form.tenants.map((tenant) => tenantPayload(tenant, form.protocol)),
    products: form.products.map((product) => ({
      name: product.name,
      product_id: product.product_id,
      path: product.path,
      method: product.method,
      weight: product.weight,
      qps_limit: product.qps_limit,
      timeout_secs: product.timeout_secs,
      query_json: parseJson(product.query_text, `产品「${product.name}」的 query`),
      headers_json: parseJson(product.headers_text, `产品「${product.name}」的 headers`),
      body_json: parseJson(product.body_text, `产品「${product.name}」的 body`),
      expect_transport: product.expect_transport,
      expect_checks_json: parseJson(product.checks_text, `产品「${product.name}」的业务校验`),
    })),
  };
}
