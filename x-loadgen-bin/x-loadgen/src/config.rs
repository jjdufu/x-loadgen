use crate::Cli;
use crate::expect::FieldCheck;
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Client;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Deserialize)]
pub struct LoadgenConfig {
    pub gateway: GatewayConfig,
    /// One or more customers; each may inline tenant fields or point at a secrets TOML.
    pub customers: Vec<CustomerConfig>,
    #[serde(default)]
    pub cluster: ClusterConfig,
    #[serde(default)]
    pub traffic: TrafficConfig,
    #[serde(default)]
    pub resolve: ResolveConfig,
    #[serde(default)]
    pub products: Vec<ProductConfig>,
    #[serde(default)]
    pub report: ReportConfig,
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
    /// Populated after load: resolved tenant profiles aligned with `customers`.
    #[serde(skip)]
    pub resolved_customers: Vec<ResolvedCustomer>,
}

/// Report / metrics selection. Metrics drive collection and which charts appear.
#[derive(Debug, Clone, Deserialize)]
pub struct ReportConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Output directory (relative → under cwd; absolute as-is).
    #[serde(default = "default_report_dir")]
    pub dir: String,
    #[serde(default = "default_report_title")]
    pub title: String,
    /// Default report UI language: `zh` or `en` (page can still toggle).
    #[serde(default = "default_report_lang")]
    pub lang: String,
    /// Profile: quick | standard | latency | ratelimit | full
    #[serde(default = "default_report_profile")]
    pub profile: String,
    /// Extra metrics to enable (merged with profile).
    #[serde(default)]
    pub metrics: Vec<String>,
    /// Metrics to disable after profile expansion.
    #[serde(default)]
    pub disable_metrics: Vec<String>,
    #[serde(default)]
    pub sla: SlaConfig,
    /// Resolved after validate (not from TOML).
    #[serde(skip)]
    pub resolved_metrics: Vec<MetricId>,
}

impl Default for ReportConfig {
    fn default() -> Self {
        let mut r = Self {
            enabled: true,
            dir: default_report_dir(),
            title: default_report_title(),
            lang: default_report_lang(),
            profile: default_report_profile(),
            metrics: Vec::new(),
            disable_metrics: Vec::new(),
            sla: SlaConfig::default(),
            resolved_metrics: Vec::new(),
        };
        r.resolved_metrics = resolve_metrics(&r.profile, &r.metrics, &r.disable_metrics)
            .unwrap_or_else(|_| profile_metrics("standard"));
        r
    }
}

impl ReportConfig {
    pub fn has_metric(&self, id: MetricId) -> bool {
        self.resolved_metrics.contains(&id)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlaConfig {
    pub success_rate_min: Option<f64>,
    pub p95_ms_max: Option<u128>,
    pub p99_ms_max: Option<u128>,
    /// HTTP 429 reasons treated as expected (e.g. `tenant_qps` for RL verification).
    #[serde(default)]
    pub expected_reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricId {
    Availability,
    Throughput,
    Latency,
    HttpStatus,
    ErrorReason,
    ReasonTimeseries,
    Bytes,
    Inflight,
    FailuresSample,
    PerProduct,
    Sla,
    ConfigSnapshot,
}

impl MetricId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Availability => "availability",
            Self::Throughput => "throughput",
            Self::Latency => "latency",
            Self::HttpStatus => "http_status",
            Self::ErrorReason => "error_reason",
            Self::ReasonTimeseries => "reason_timeseries",
            Self::Bytes => "bytes",
            Self::Inflight => "inflight",
            Self::FailuresSample => "failures_sample",
            Self::PerProduct => "per_product",
            Self::Sla => "sla",
            Self::ConfigSnapshot => "config_snapshot",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "availability" => Some(Self::Availability),
            "throughput" => Some(Self::Throughput),
            "latency" => Some(Self::Latency),
            "http_status" => Some(Self::HttpStatus),
            "error_reason" => Some(Self::ErrorReason),
            "reason_timeseries" => Some(Self::ReasonTimeseries),
            "bytes" => Some(Self::Bytes),
            "inflight" => Some(Self::Inflight),
            "failures_sample" => Some(Self::FailuresSample),
            "per_product" => Some(Self::PerProduct),
            "sla" => Some(Self::Sla),
            "config_snapshot" => Some(Self::ConfigSnapshot),
            _ => None,
        }
    }
}

fn profile_metrics(profile: &str) -> Vec<MetricId> {
    use MetricId::{
        Availability, Bytes, ConfigSnapshot, ErrorReason, FailuresSample, HttpStatus, Latency,
        PerProduct, ReasonTimeseries, Sla, Throughput,
    };
    match profile.trim().to_ascii_lowercase().as_str() {
        "quick" => vec![
            Availability,
            Latency,
            HttpStatus,
            ConfigSnapshot,
            FailuresSample,
        ],
        "latency" => vec![
            Availability,
            Throughput,
            Latency,
            HttpStatus,
            PerProduct,
            FailuresSample,
            ConfigSnapshot,
            Sla,
        ],
        "ratelimit" => vec![
            Availability,
            Throughput,
            HttpStatus,
            ErrorReason,
            ReasonTimeseries,
            PerProduct,
            FailuresSample,
            ConfigSnapshot,
            Sla,
        ],
        "full" => vec![
            Availability,
            Throughput,
            Latency,
            HttpStatus,
            ErrorReason,
            ReasonTimeseries,
            Bytes,
            FailuresSample,
            PerProduct,
            Sla,
            ConfigSnapshot,
        ],
        // standard (default)
        _ => vec![
            Availability,
            Throughput,
            Latency,
            HttpStatus,
            ErrorReason,
            PerProduct,
            FailuresSample,
            ConfigSnapshot,
        ],
    }
}

pub fn resolve_metrics(
    profile: &str,
    extra: &[String],
    disable: &[String],
) -> Result<Vec<MetricId>> {
    let mut set: Vec<MetricId> = profile_metrics(profile);
    for m in extra {
        let id = MetricId::parse(m).ok_or_else(|| anyhow!("unknown report metric id: {m}"))?;
        if !set.contains(&id) {
            set.push(id);
        }
    }
    for m in disable {
        let id =
            MetricId::parse(m).ok_or_else(|| anyhow!("unknown report disable_metrics id: {m}"))?;
        set.retain(|x| *x != id);
    }
    // Always keep config snapshot for professional reports when report enabled
    if !set.contains(&MetricId::ConfigSnapshot) {
        set.push(MetricId::ConfigSnapshot);
    }
    Ok(set)
}

#[derive(Debug, Clone, Deserialize)]
pub struct TenantConfig {
    pub api_key: String,
    /// `none` | `request` | `full` — 对齐 mazbri 租户传输 `TransportMode`。
    #[serde(default = "default_encryption_mode")]
    pub encryption_mode: String,
    /// 平台租户 UUID（加密 AAD）；`request`/`full` 必填。
    #[serde(default)]
    pub tenant_id: String,
    /// 租户数据面密钥 Base64（`create_crypto_key` 返回的 `tenantSecretKey`）。
    #[serde(default)]
    pub tenant_key_base64: String,
    #[serde(default = "default_key_version")]
    pub key_version: String,
    /// 命名套件，如 `suite/tenant-default-2026-11`；`request`/`full` 时与 `data_alg` 至少给一个。
    #[serde(default)]
    pub suite: String,
    /// 数据算法（kebab-case）；未用命名套件的租户（自定义组合）必填，与 suite 同给时必须一致。
    #[serde(default)]
    pub data_alg: Option<String>,
    /// 可选覆盖 MAC；AEAD 套件应留空。
    #[serde(default)]
    pub mac_alg: Option<String>,
    /// 未设置时：套件含 `mac_alg` 则为 true，否则 false。
    #[serde(default)]
    pub require_signature: Option<bool>,
    /// 是否允许 Legacy 算法（默认 false）。
    #[serde(default)]
    pub allow_insecure: bool,
    /// 可选请求绑定（`tenant_request_keys`）；租户 `require_request_binding=true` 时必配。
    #[serde(default)]
    pub binding: Option<crate::binding::BindingConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    /// e.g. `http://gateway.example:8888` — token/proxy paths are derived.
    pub base_url: String,
    /// Optional client certificate (PEM) for mTLS to the gateway.
    #[serde(default)]
    pub client_cert: Option<String>,
    /// Optional client private key (PEM) for mTLS.
    #[serde(default)]
    pub client_key: Option<String>,
    /// Optional CA bundle (PEM) to trust.
    #[serde(default)]
    pub ca_cert: Option<String>,
    /// When true, do not validate server certs (lab only).
    #[serde(default)]
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClusterConfig {
    /// Number of OS processes to spawn (1 = in-process only).
    #[serde(default = "default_processes")]
    pub processes: usize,
    /// How to split `traffic.qps` across processes: `total` or `per_process`.
    #[serde(default = "default_qps_mode")]
    pub qps_mode: String,
    /// How to split `traffic.requests` across processes: `total` or `per_process`.
    #[serde(default = "default_qps_mode")]
    pub requests_mode: String,
}

fn default_processes() -> usize {
    1
}

fn default_qps_mode() -> String {
    "total".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ResolveConfig {
    /// TOML map: `[products."CHANNEL"] { product_id, path? }`.
    #[serde(default)]
    pub product_map: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EnrichmentConfig {
    #[serde(default)]
    pub clickhouse: crate::enrich::ClickHouseEnrichment,
}

/// A customer identity for the load. Prefer `secrets` pointing at a TOML that
/// contains `[tenant]` (+ optional `[tenant.binding]`); inline fields also work.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomerConfig {
    pub name: String,
    /// Path to secrets TOML (`[tenant]` section). Relative to the main config file.
    #[serde(default)]
    pub secrets: Option<String>,
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Inline tenant — used when `secrets` is absent; also overlays secrets when both set
    /// is **not** supported: exactly one of `secrets` or inline `api_key`.
    #[serde(flatten)]
    pub inline: InlineTenant,
}

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct InlineTenant {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub encryption_mode: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub tenant_key_base64: Option<String>,
    #[serde(default)]
    pub key_version: Option<String>,
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub data_alg: Option<String>,
    #[serde(default)]
    pub mac_alg: Option<String>,
    #[serde(default)]
    pub require_signature: Option<bool>,
    #[serde(default)]
    pub allow_insecure: Option<bool>,
    #[serde(default)]
    pub binding: Option<crate::binding::BindingConfig>,
}

#[derive(Debug, Clone)]
pub struct ResolvedCustomer {
    pub name: String,
    pub weight: u32,
    pub tenant: TenantConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrafficConfig {
    /// Traffic model: `open_loop` (pace by QPS) or `closed_loop` (keep concurrency full).
    #[serde(default = "default_traffic_model")]
    pub model: String,
    /// Total requests for this process (after cluster split). Mutually exclusive with `duration_secs`.
    #[serde(default)]
    pub requests: Option<u32>,
    /// Run duration in seconds (open/closed). Mutually exclusive with `requests`.
    #[serde(default)]
    pub duration_secs: Option<u64>,
    /// Warmup seconds (not counted toward report window when enrichment is used).
    #[serde(default)]
    pub warmup_secs: u64,
    /// Linear ramp to target QPS/concurrency over this many seconds.
    #[serde(default)]
    pub ramp_secs: u64,
    /// Global QPS cap for `open_loop`. When unset (and product has no `qps_limit`), unbounded
    /// subject only to concurrency.
    #[serde(default)]
    pub qps: Option<u32>,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    /// Attach HTTP `x-request-id` (not injected into proxy JSON body).
    #[serde(default = "default_true")]
    pub include_request_id: bool,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    #[serde(default = "default_log_file")]
    pub log_file: String,
    /// Refresh access token this many seconds before expiry.
    #[serde(default = "default_token_refresh_skew_secs")]
    pub token_refresh_skew_secs: u64,
    /// When true, do not keep idle HTTP connections (`pool_max_idle_per_host=0`).
    /// Used to diagnose keep-alive / connection-reuse races.
    #[serde(default)]
    pub disable_connection_pool: bool,
    /// Optional override for reqwest pool idle timeout (seconds). None = reqwest default.
    #[serde(default)]
    pub pool_idle_timeout_secs: Option<u64>,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            model: default_traffic_model(),
            requests: Some(default_requests_per_product()),
            duration_secs: None,
            warmup_secs: 0,
            ramp_secs: 0,
            qps: None,
            concurrency: default_concurrency(),
            timeout_secs: default_timeout_secs(),
            connect_timeout_secs: default_connect_timeout_secs(),
            include_request_id: default_true(),
            user_agent: default_user_agent(),
            log_dir: default_log_dir(),
            log_file: default_log_file(),
            token_refresh_skew_secs: default_token_refresh_skew_secs(),
            disable_connection_pool: false,
            pool_idle_timeout_secs: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductConfig {
    pub name: String,
    /// Gateway product id; optional when `channel_code` + `[resolve].product_map` is set.
    #[serde(default)]
    pub product_id: String,
    /// Stable channel code resolved via `[resolve].product_map` at load time.
    #[serde(default)]
    pub channel_code: Option<String>,
    #[serde(default)]
    pub path: String,
    pub method: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub qps_limit: Option<u32>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    /// Only these headers are placed into the proxy request JSON `headers` map.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body_json: Option<String>,
    /// Load body template from file (UTF-8). Supports the same `{{...}}` placeholders as `body_json`.
    #[serde(default)]
    pub body_file: Option<String>,
    #[serde(default)]
    pub body_image_dir: Option<String>,
    #[serde(default)]
    pub body_image_field: Option<String>,
    /// When set, each picked image also writes its file name into this body field.
    #[serde(default)]
    pub body_image_name_field: Option<String>,
    /// `ok` (default) or `reject`. `reject` means a non-2xx gateway call is the expected result.
    #[serde(default = "default_expect_transport")]
    pub expect_transport: String,
    #[serde(default)]
    pub expect_checks: Vec<FieldCheck>,
}

fn default_expect_transport() -> String {
    "ok".into()
}

impl TenantConfig {
    pub fn is_encryption_none(&self) -> bool {
        self.encryption_mode.eq_ignore_ascii_case("none")
    }

    pub fn is_encryption_request(&self) -> bool {
        self.encryption_mode.eq_ignore_ascii_case("request")
    }

    pub fn is_encryption_full(&self) -> bool {
        self.encryption_mode.eq_ignore_ascii_case("full")
    }

    pub fn is_encryption_enabled(&self) -> bool {
        self.is_encryption_request() || self.is_encryption_full()
    }

    pub fn tenant_key_bytes(&self) -> Result<Vec<u8>> {
        let key = STANDARD
            .decode(self.tenant_key_base64.trim())
            .context("failed to base64 decode tenant key")?;
        if key.len() != mazbri_core::crypto::transport::TENANT_SECRET_KEY_BYTES {
            return Err(anyhow!(
                "tenant key must be {} bytes, got {}",
                mazbri_core::crypto::transport::TENANT_SECRET_KEY_BYTES,
                key.len()
            ));
        }
        Ok(key)
    }

    /// 由 `[tenant.binding]` 构造请求绑定签名器（未配置返回 `None`）。
    pub fn request_signer(&self) -> Result<Option<mazbri_sdk::RequestSigner>> {
        self.binding
            .as_ref()
            .map(crate::binding::BindingConfig::signer)
            .transpose()
    }

    pub async fn fetch_token(&self, http: &Client, token_url: &str) -> Result<FetchedToken> {
        let response = http
            .post(token_url)
            .json(&serde_json::json!({
                "api_key": self.api_key,
                "grant_type": "client_credentials"
            }))
            .send()
            .await
            .context("failed to request token")?;
        let status = response.status();
        let body = response.text().await.context("failed to read token body")?;
        let token: TokenBody = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse token response (HTTP {status}): {body}"))?;
        if !token.success {
            return Err(anyhow!(
                "token request failed (HTTP {}): {}",
                status,
                token.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }
        let access_token = token
            .access_token
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("token response did not include access_token"))?;
        let expires_in = token.expires_in.unwrap_or(1800);
        Ok(FetchedToken {
            access_token,
            expires_in,
            token_type: token.token_type.unwrap_or_else(|| "Bearer".to_string()),
        })
    }

    pub(crate) fn validate(&self, label: &str) -> Result<()> {
        if self.api_key.trim().is_empty() {
            anyhow::bail!("{label}: api_key is required");
        }
        if self.is_encryption_enabled() {
            if self.tenant_id.trim().is_empty() {
                anyhow::bail!("{label}: tenant_id is required when encryption_mode=request|full");
            }
            if self.tenant_key_base64.trim().is_empty() {
                anyhow::bail!(
                    "{label}: tenant_key_base64 is required when encryption_mode=request|full"
                );
            }
            if self.suite.trim().is_empty()
                && self.data_alg.as_deref().is_none_or(|s| s.trim().is_empty())
            {
                anyhow::bail!(
                    "{label}: suite or data_alg is required when encryption_mode=request|full"
                );
            }
            if self.key_version.trim().is_empty() {
                anyhow::bail!("{label}: key_version is required when encryption_mode=request|full");
            }
            let _ = self.tenant_key_bytes()?;
        } else if !self.is_encryption_none() {
            anyhow::bail!("{label}: encryption_mode must be none, request, or full");
        }
        let _ = self.request_signer()?;
        Ok(())
    }
}

impl LoadgenConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read loadgen config: {}", path.display()))?;
        let mut config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse loadgen config: {}", path.display()))?;
        config.resolved_customers = resolve_customers(&config.customers, base)?;
        for customer in &mut config.resolved_customers {
            if let Some(binding) = customer.tenant.binding.as_mut() {
                binding.resolve_paths(base);
            }
        }
        if let Some(map) = config.resolve.product_map.clone() {
            let map_path = resolve_config_path(base, &map);
            crate::resolve::apply_product_map(&mut config.products, &map_path)?;
        }
        for product in &mut config.products {
            if let Some(file) = product.body_file.clone() {
                let body_path = resolve_config_path(base, &file);
                let body = fs::read_to_string(&body_path)
                    .with_context(|| format!("failed to read body_file {}", body_path.display()))?;
                if product
                    .body_json
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty())
                {
                    anyhow::bail!("product {} sets both body_json and body_file", product.name);
                }
                product.body_json = Some(body);
            }
            if let Some(dir) = product.body_image_dir.as_mut() {
                *dir = resolve_config_path(base, dir)
                    .to_string_lossy()
                    .into_owned();
            }
        }
        config.validate()?;
        config.report.resolved_metrics = resolve_metrics(
            &config.report.profile,
            &config.report.metrics,
            &config.report.disable_metrics,
        )?;
        Ok(config)
    }

    pub fn apply_cli_overrides(&mut self, cli: &Cli) {
        if let Some(qps) = cli.qps {
            self.traffic.qps = Some(qps);
        }
        if let Some(concurrency) = cli.concurrency {
            self.traffic.concurrency = concurrency;
        }
        if let Some(requests) = cli.requests {
            self.traffic.requests = Some(requests);
            self.traffic.duration_secs = None;
        }
        if let Some(duration) = cli.duration_secs {
            self.traffic.duration_secs = Some(duration);
            self.traffic.requests = None;
        }
        if let Some(processes) = cli.processes {
            self.cluster.processes = processes;
        }
        if cli.disable_connection_pool {
            self.traffic.disable_connection_pool = true;
        }
        if let Some(secs) = cli.pool_idle_timeout_secs {
            self.traffic.pool_idle_timeout_secs = Some(secs);
        }
        if let Some(ref only) = cli.only_product {
            let needle = only.trim().to_ascii_lowercase();
            self.products.retain(|p| {
                p.product_id.eq_ignore_ascii_case(only)
                    || p.name.to_ascii_lowercase().contains(&needle)
                    || p.channel_code
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case(only))
                    || p.path
                        .trim_matches('/')
                        .eq_ignore_ascii_case(only.trim_matches('/'))
            });
        }
        if cli.no_report {
            self.report.enabled = false;
        }
        if let Some(ref dir) = cli.report_dir {
            self.report.dir.clone_from(dir);
        }
        if let Some(ref title) = cli.report_title {
            self.report.title.clone_from(title);
        }
        if let Some(ref lang) = cli.report_lang {
            self.report.lang.clone_from(lang);
        }
        if let Some(ref profile) = cli.report_profile {
            self.report.profile.clone_from(profile);
        }
        if let Some(ref metrics) = cli.report_metrics {
            self.report.metrics = metrics
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Some(ref reasons) = cli.expected_reasons {
            self.report.sla.expected_reasons = reasons
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(resolved) = resolve_metrics(
            &self.report.profile,
            &self.report.metrics,
            &self.report.disable_metrics,
        ) {
            self.report.resolved_metrics = resolved;
        }
    }

    /// Split total QPS/requests across worker processes when modes are `total`.
    pub fn apply_worker_split(&mut self, worker_index: usize, worker_count: usize) {
        if worker_count <= 1 {
            return;
        }
        let wc = crate::num::usize_to_u32(worker_count).max(1);
        let wi = crate::num::usize_to_u32(worker_index);
        if self.cluster.qps_mode.eq_ignore_ascii_case("total")
            && let Some(qps) = self.traffic.qps
        {
            let base = qps / wc;
            let rem = qps % wc;
            self.traffic.qps = Some(base + u32::from(wi < rem));
        }
        if self.cluster.requests_mode.eq_ignore_ascii_case("total")
            && let Some(requests) = self.traffic.requests
        {
            let base = requests / wc;
            let rem = requests % wc;
            self.traffic.requests = Some(base + u32::from(wi < rem));
        }
    }

    pub fn report_output_dir(&self, run_id: &str) -> PathBuf {
        PathBuf::from(&self.report.dir).join(run_id)
    }

    pub fn init_logging(&self) -> Result<WorkerGuard> {
        let log_dir_buf = if Path::new(&self.traffic.log_dir).is_absolute() {
            PathBuf::from(&self.traffic.log_dir)
        } else {
            PathBuf::from("/tmp").join(&self.traffic.log_dir)
        };
        let log_dir = log_dir_buf.as_path();
        fs::create_dir_all(log_dir)
            .with_context(|| format!("failed to create log directory: {}", log_dir.display()))?;
        let file_appender = tracing_appender::rolling::never(log_dir, &self.traffic.log_file);
        let (writer, guard) = tracing_appender::non_blocking(file_appender);
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(writer)
            .with_ansi(false)
            .json()
            .flatten_event(true)
            .try_init()
            .ok();
        Ok(guard)
    }

    pub fn gateway_base_url(&self) -> String {
        self.gateway
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_string()
    }

    pub fn token_url(&self) -> String {
        format!("{}/auth/token", self.gateway_base_url())
    }

    pub fn invoke_url(&self) -> String {
        format!("{}/api/v1/gateway", self.gateway_base_url())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.validate_gateway()?;
        if self.customers.is_empty() {
            anyhow::bail!("at least one [[customers]] entry is required");
        }
        if self.resolved_customers.is_empty() {
            anyhow::bail!("no customers resolved from [[customers]]");
        }
        for (i, c) in self.resolved_customers.iter().enumerate() {
            if c.weight == 0 {
                anyhow::bail!("customers[{i}] ({}) weight must be > 0", c.name);
            }
            c.tenant.validate(&format!("customers[{i}] ({})", c.name))?;
        }
        self.validate_traffic()?;
        self.validate_cluster()?;
        self.validate_report()?;
        for product in &self.products {
            validate_product(product)?;
        }
        Ok(())
    }

    fn validate_gateway(&self) -> Result<()> {
        if self.gateway.base_url.trim().is_empty() {
            anyhow::bail!("gateway.base_url is required");
        }
        let base = self.gateway.base_url.trim();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            anyhow::bail!("gateway.base_url must start with http:// or https://");
        }
        let cert = self.gateway.client_cert.as_deref().unwrap_or("").trim();
        let key = self.gateway.client_key.as_deref().unwrap_or("").trim();
        if cert.is_empty() != key.is_empty() {
            anyhow::bail!("gateway.client_cert and gateway.client_key must be set together");
        }
        Ok(())
    }

    fn validate_traffic(&self) -> Result<()> {
        let model = self.traffic.model.trim().to_ascii_lowercase();
        if !matches!(model.as_str(), "open_loop" | "closed_loop") {
            anyhow::bail!("traffic.model must be open_loop or closed_loop");
        }
        if self.traffic.timeout_secs == 0 {
            anyhow::bail!("traffic.timeout_secs must be greater than 0");
        }
        if self.traffic.connect_timeout_secs == 0 {
            anyhow::bail!("traffic.connect_timeout_secs must be greater than 0");
        }
        if self.traffic.concurrency == 0 {
            anyhow::bail!("traffic.concurrency must be greater than 0");
        }
        match (self.traffic.requests, self.traffic.duration_secs) {
            (None, None) => {
                anyhow::bail!("traffic.requests or traffic.duration_secs is required");
            }
            (Some(0), _) => anyhow::bail!("traffic.requests must be greater than 0 when set"),
            (_, Some(0)) => anyhow::bail!("traffic.duration_secs must be greater than 0 when set"),
            (Some(_), Some(_)) => {
                anyhow::bail!("set only one of traffic.requests or traffic.duration_secs");
            }
            _ => {}
        }
        if let Some(0) = self.traffic.qps {
            anyhow::bail!("traffic.qps must be greater than 0 when set");
        }
        if model == "open_loop" && self.traffic.qps.is_none() {
            // allowed: unbounded open loop paced only by concurrency completion
        }
        Ok(())
    }

    fn validate_cluster(&self) -> Result<()> {
        if self.cluster.processes == 0 {
            anyhow::bail!("cluster.processes must be greater than 0");
        }
        for (field, value) in [
            ("qps_mode", self.cluster.qps_mode.as_str()),
            ("requests_mode", self.cluster.requests_mode.as_str()),
        ] {
            let v = value.trim().to_ascii_lowercase();
            if !matches!(v.as_str(), "total" | "per_process") {
                anyhow::bail!("cluster.{field} must be total or per_process");
            }
        }
        Ok(())
    }

    fn validate_report(&self) -> Result<()> {
        let resolved = resolve_metrics(
            &self.report.profile,
            &self.report.metrics,
            &self.report.disable_metrics,
        )?;
        let _ = resolved;
        let profile = self.report.profile.trim().to_ascii_lowercase();
        if !matches!(
            profile.as_str(),
            "quick" | "standard" | "latency" | "ratelimit" | "full"
        ) {
            anyhow::bail!(
                "report.profile must be one of: quick, standard, latency, ratelimit, full"
            );
        }
        let lang = self.report.lang.trim().to_ascii_lowercase();
        if !matches!(lang.as_str(), "zh" | "en" | "zh-cn" | "en-us") {
            anyhow::bail!("report.lang must be zh or en");
        }
        for m in &self.report.metrics {
            if MetricId::parse(m).is_none() {
                anyhow::bail!("unknown report.metrics entry: {m}");
            }
        }
        for m in &self.report.disable_metrics {
            if MetricId::parse(m).is_none() {
                anyhow::bail!("unknown report.disable_metrics entry: {m}");
            }
        }
        if let Some(rate) = self.report.sla.success_rate_min
            && !(0.0..=1.0).contains(&rate)
        {
            anyhow::bail!("report.sla.success_rate_min must be between 0 and 1");
        }
        Ok(())
    }
}

fn resolve_config_path(base: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

#[derive(Debug, Deserialize)]
struct SecretsFile {
    tenant: TenantConfig,
}

fn resolve_customers(customers: &[CustomerConfig], base: &Path) -> Result<Vec<ResolvedCustomer>> {
    let mut out = Vec::with_capacity(customers.len());
    for (i, c) in customers.iter().enumerate() {
        let name = c.name.trim();
        if name.is_empty() {
            anyhow::bail!("customers[{i}].name is required");
        }
        let tenant = if let Some(secrets) = c.secrets.as_deref().filter(|s| !s.trim().is_empty()) {
            if c.inline
                .api_key
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty())
            {
                anyhow::bail!(
                    "customers[{i}] ({name}): set either secrets= or inline api_key, not both"
                );
            }
            let path = resolve_config_path(base, secrets);
            let raw = fs::read_to_string(&path).with_context(|| {
                format!("customers[{i}] ({name}): read secrets {}", path.display())
            })?;
            let file: SecretsFile = toml::from_str(&raw).with_context(|| {
                format!("customers[{i}] ({name}): parse secrets {}", path.display())
            })?;
            file.tenant
        } else {
            let api_key = c
                .inline
                .api_key
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    anyhow!("customers[{i}] ({name}): secrets= or api_key is required")
                })?;
            TenantConfig {
                api_key,
                encryption_mode: c
                    .inline
                    .encryption_mode
                    .clone()
                    .unwrap_or_else(default_encryption_mode),
                tenant_id: c.inline.tenant_id.clone().unwrap_or_default(),
                tenant_key_base64: c.inline.tenant_key_base64.clone().unwrap_or_default(),
                key_version: c
                    .inline
                    .key_version
                    .clone()
                    .unwrap_or_else(default_key_version),
                suite: c.inline.suite.clone().unwrap_or_default(),
                data_alg: c.inline.data_alg.clone(),
                mac_alg: c.inline.mac_alg.clone(),
                require_signature: c.inline.require_signature,
                allow_insecure: c.inline.allow_insecure.unwrap_or(false),
                binding: c.inline.binding.clone(),
            }
        };
        out.push(ResolvedCustomer {
            name: name.to_string(),
            weight: c.weight,
            tenant,
        });
    }
    Ok(out)
}

fn validate_product(product: &ProductConfig) -> Result<()> {
    if product.name.trim().is_empty() {
        anyhow::bail!("product.name is required");
    }
    if product.product_id.trim().is_empty() {
        anyhow::bail!(
            "product.product_id is required for product {} (set it or resolve via channel_code + [resolve].product_map)",
            product.name
        );
    }
    if product.path.trim().is_empty() {
        anyhow::bail!("product.path is required for product {}", product.name);
    }
    if product.method.trim().is_empty() {
        anyhow::bail!("product.method is required for product {}", product.name);
    }
    if product.weight == 0 {
        anyhow::bail!("product.weight must be > 0 for product {}", product.name);
    }
    if product.timeout_secs == Some(0) {
        anyhow::bail!(
            "product.timeout_secs must be greater than 0 when set for product {}",
            product.name
        );
    }
    if let Some(0) = product.qps_limit {
        anyhow::bail!(
            "product.qps_limit must be greater than 0 when set for product {}",
            product.name
        );
    }
    let has_dir = product
        .body_image_dir
        .as_deref()
        .map(str::trim)
        .as_ref()
        .is_some_and(|v| !v.is_empty());
    let has_field = product
        .body_image_field
        .as_deref()
        .map(str::trim)
        .as_ref()
        .is_some_and(|v| !v.is_empty());
    if has_dir != has_field {
        anyhow::bail!(
            "product {} must set both body_image_dir and body_image_field, or neither",
            product.product_id
        );
    }
    let has_name = product
        .body_image_name_field
        .as_deref()
        .map(str::trim)
        .as_ref()
        .is_some_and(|v| !v.is_empty());
    if has_name && !has_dir {
        anyhow::bail!(
            "product {} sets body_image_name_field without an image pool",
            product.product_id
        );
    }
    if crate::expect::TransportExpect::parse(&product.expect_transport).is_none() {
        anyhow::bail!(
            "product {} expect_transport must be ok or reject",
            product.product_id
        );
    }
    if product.expect_transport.eq_ignore_ascii_case("reject") && !product.expect_checks.is_empty()
    {
        anyhow::bail!(
            "product {} expect_transport=reject cannot also set expect_checks",
            product.product_id
        );
    }
    for check in &product.expect_checks {
        if check.path.trim().is_empty() {
            anyhow::bail!(
                "product {} has an expect_checks entry with an empty path",
                product.product_id
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct FetchedToken {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct TokenBody {
    success: bool,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_traffic_model() -> String {
    "open_loop".into()
}

fn default_requests_per_product() -> u32 {
    100
}

fn default_concurrency() -> usize {
    64
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_connect_timeout_secs() -> u64 {
    10
}

fn default_user_agent() -> String {
    format!("x-loadgen/{}", env!("CARGO_PKG_VERSION"))
}

fn default_log_dir() -> String {
    "logs/loadgen".to_string()
}

fn default_log_file() -> String {
    "x-loadgen.log".to_string()
}

fn default_key_version() -> String {
    "v1".to_string()
}

fn default_encryption_mode() -> String {
    "none".to_string()
}

fn default_token_refresh_skew_secs() -> u64 {
    60
}

fn default_report_dir() -> String {
    "reports".to_string()
}

fn default_report_title() -> String {
    "Mazbri Gateway Load Test".to_string()
}

fn default_report_lang() -> String {
    "zh".to_string()
}

fn default_report_profile() -> String {
    "standard".to_string()
}

/// Shared HTTP client builder used for token + plain proxy.
pub fn build_http_client(config: &LoadgenConfig) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(config.traffic.connect_timeout_secs))
        .timeout(Duration::from_secs(config.traffic.timeout_secs))
        .user_agent(config.traffic.user_agent.clone())
        .tcp_nodelay(true);
    if config.traffic.disable_connection_pool {
        // Force a new connection for every request (no idle keep-alive reuse).
        builder = builder.pool_max_idle_per_host(0);
    }
    if let Some(secs) = config.traffic.pool_idle_timeout_secs {
        builder = builder.pool_idle_timeout(Duration::from_secs(secs));
    }
    let cert = config
        .gateway
        .client_cert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let key = config
        .gateway
        .client_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let (Some(cert_path), Some(key_path)) = (cert, key) {
        let cert_pem =
            fs::read(cert_path).with_context(|| format!("read gateway.client_cert {cert_path}"))?;
        let key_pem =
            fs::read(key_path).with_context(|| format!("read gateway.client_key {key_path}"))?;
        let mut identity_pem = cert_pem;
        identity_pem.extend_from_slice(&key_pem);
        let identity = reqwest::Identity::from_pem(&identity_pem)
            .context("failed to load mTLS client identity (PEM cert+key)")?;
        builder = builder.identity(identity);
    }
    if let Some(ca) = config
        .gateway
        .ca_cert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let pem = fs::read(ca).with_context(|| format!("read gateway.ca_cert {ca}"))?;
        let cert =
            reqwest::Certificate::from_pem(&pem).context("failed to parse gateway.ca_cert PEM")?;
        builder = builder.add_root_certificate(cert);
    }
    if config.gateway.insecure_skip_verify {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder.build().context("failed to build reqwest client")
}
