use crate::config::{
    FetchedToken, LoadgenConfig, ProductConfig, ResolvedCustomer, TenantConfig, build_http_client,
};
use crate::expect::{self, TransportExpect};
use crate::metrics::{
    MetricsCollector, RequestSample, RunContext, is_expected_reason, unix_now_ms,
};
use crate::report;
use crate::template;
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures::stream::{FuturesUnordered, StreamExt};
use mazbri_core::crypto::{AlgorithmId, KeySelection, TenantTransportProfile, TransportMode};
use mazbri_sdk::{
    ClientConfig, GATEWAY_INVOKE_PATH, GatewayResponseEnvelope, MazbriClient, RequestSigner,
};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{sleep_until, timeout};
use tracing::{error, info};
use uuid::Uuid;

/// Customer-facing proxy request body (no tool-only fields).
#[derive(Debug, Clone, Serialize)]
struct GatewayRequest {
    product_id: String,
    path: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<Value>,
}

#[derive(Debug, Clone)]
struct RequestOutcome {
    request_index: u32,
    http_status: u16,
    proxy_status: Option<u16>,
    success: bool,
    latency_ms: u128,
    response_bytes: usize,
    error: Option<String>,
    /// Gateway rate-limit / error reason when present (e.g. `tenant_qps`).
    reason: Option<String>,
    error_class: Option<String>,
    request_id: Option<String>,
    start_unix_ms: u64,
    end_unix_ms: u64,
}

#[derive(Debug, Clone)]
struct ProductSummary {
    name: String,
    product_id: String,
    requests: u32,
    success: u32,
    failure: u32,
    /// Failures that matched `report.sla.expected_reasons` (e.g. `tenant_qps`).
    expected_reject: u32,
    latencies_ms: Vec<u128>,
    /// HTTP status code histogram (includes transport failures as 0 when unparsed).
    status_counts: HashMap<u16, u32>,
    /// Rate-limit / error reason histogram from gateway error body (e.g. `tenant_qps`).
    reason_counts: HashMap<String, u32>,
}

struct CachedToken {
    access_token: String,
    /// Instant after which token should be refreshed.
    refresh_after: Instant,
}

struct PlainTransport {
    customer_name: String,
    http: Client,
    signer: Option<RequestSigner>,
    token: Mutex<CachedToken>,
    invoke_url: String,
    token_url: String,
    tenant: TenantConfig,
    token_refresh_skew_secs: u64,
}

/// Holds a concurrent-safe [`MazbriClient`] (`&self` RPCs, per-request nonce).
struct EncryptedTransport {
    customer_name: String,
    client: MazbriClient,
}

enum ProxyTransport {
    Plain(Arc<PlainTransport>),
    Encrypted(Arc<EncryptedTransport>),
}

impl Clone for ProxyTransport {
    fn clone(&self) -> Self {
        match self {
            Self::Plain(inner) => Self::Plain(Arc::clone(inner)),
            Self::Encrypted(inner) => Self::Encrypted(Arc::clone(inner)),
        }
    }
}

/// Mix request index (or any seed) before weight modulo.
/// Plain `seed * 0x9E3779B9` is 0 mod 3 and collapses equal 3-way weights.
fn mix_request_seed(seed: u64) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(31) ^ seed.wrapping_shr(17)
}

struct TransportPool {
    /// `(cumulative_weight_end, transport)`
    entries: Vec<(u32, ProxyTransport)>,
    total_weight: u32,
}

impl TransportPool {
    fn pick(&self, seed: u64) -> &ProxyTransport {
        debug_assert!(self.total_weight > 0);
        let r = crate::num::u64_to_u32(mix_request_seed(seed) % u64::from(self.total_weight))
            .saturating_add(1);
        for (end, t) in &self.entries {
            if r <= *end {
                return t;
            }
        }
        &self.entries.last().expect("non-empty pool").1
    }
}

async fn maybe_write_report(
    config: &LoadgenConfig,
    collector: &Mutex<MetricsCollector>,
    run_id: &str,
) {
    if !config.report.enabled {
        return;
    }
    let title = config.report.title.clone();
    let encryption = config
        .resolved_customers
        .iter()
        .map(|c| format!("{}:{}", c.name, c.tenant.encryption_mode))
        .collect::<Vec<_>>()
        .join(",");
    let run_context = RunContext {
        tool: format!("x-loadgen/{}", env!("CARGO_PKG_VERSION")),
        gateway: config.gateway_base_url(),
        encryption,
        concurrency: config.traffic.concurrency,
        requests_per_product: config.traffic.requests.unwrap_or(0),
        qps: config.traffic.qps,
        products: config.products.len(),
        labels: config
            .products
            .iter()
            .map(|p| format!("{} ({})", p.name, p.product_id))
            .collect(),
    };
    let summary = collector
        .lock()
        .await
        .build_summary(run_id, &title, run_context);
    let out_dir = config.report_output_dir(run_id);
    let echarts = report_echarts_path();
    match report::write_report(&out_dir, &summary, &echarts, &config.report.lang) {
        Ok((html, json)) => {
            println!("report: {}", html.display());
            println!("data:   {}", json.display());
            info!(
                report_html = %html.display(),
                summary_json = %json.display(),
                verdict = %summary.verdict.status,
                "report generated"
            );
        }
        Err(err) => {
            error!(error = ?err, "failed to write report");
        }
    }
}

async fn maybe_enrich(
    config: &LoadgenConfig,
    collector: &Mutex<MetricsCollector>,
    run_id: &str,
    window_from_ms: u64,
) {
    if !config.enrichment.clickhouse.enabled {
        return;
    }
    let request_ids = collector.lock().await.request_ids();
    let out_dir = config.report_output_dir(run_id);
    match crate::enrich::run_clickhouse_enrichment(
        &config.enrichment.clickhouse,
        &crate::enrich::EnrichmentWindow {
            from_ms: window_from_ms.saturating_add(config.traffic.warmup_secs.saturating_mul(1000)),
            to_ms: unix_now_ms(),
            request_ids,
        },
        &out_dir,
    ) {
        Ok(Some(path)) => println!("enrichment: {}", path.display()),
        Ok(None) => {}
        Err(err) => error!(error = ?err, "clickhouse enrichment failed"),
    }
}

pub async fn run_load_test(config: LoadgenConfig, run_id: Option<String>) -> Result<()> {
    let pool = Arc::new(build_transport_pool(&config).await?);
    let started_at = Instant::now();
    let window_from_ms = unix_now_ms();
    let collector = Arc::new(Mutex::new(MetricsCollector::new(config.report.clone())));
    let config = Arc::new(config);
    let overall = run_products_parallel(
        Arc::clone(&config),
        Arc::clone(&pool),
        Arc::clone(&collector),
    )
    .await?;

    let elapsed = started_at.elapsed().as_secs_f64();
    let total_requests: u32 = overall.iter().map(|s| s.requests).sum();
    let total_success: u32 = overall.iter().map(|s| s.success).sum();
    let total_failure: u32 = overall.iter().map(|s| s.failure).sum();

    info!(
        total_requests,
        total_success,
        total_failure,
        elapsed_seconds = elapsed,
        "load test completed"
    );

    print_human_summary(
        &overall,
        total_requests,
        total_success,
        total_failure,
        elapsed,
    )?;

    let run_id = run_id.unwrap_or_else(|| {
        let ms = unix_now_ms();
        let short = &Uuid::new_v4().to_string()[..8];
        format!("{ms}-{short}")
    });
    maybe_write_report(&config, &collector, &run_id).await;
    maybe_enrich(&config, &collector, &run_id, window_from_ms).await;

    let unexpected_failure: u32 = overall
        .iter()
        .map(|s| s.failure.saturating_sub(s.expected_reject))
        .sum();
    if unexpected_failure > 0 {
        anyhow::bail!(
            "load test finished with failures: {unexpected_failure} unexpected / {total_requests} total (raw_failure={total_failure})"
        );
    }
    Ok(())
}

/// 按产品并行压测（小包并行），用信号量限制同时产品数。
async fn run_products_parallel(
    config: Arc<LoadgenConfig>,
    pool: Arc<TransportPool>,
    collector: Arc<Mutex<MetricsCollector>>,
) -> Result<Vec<ProductSummary>> {
    let product_parallel = std::env::var("X_LOADGEN_PRODUCT_PARALLEL")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8)
        .max(1);
    let gate = Arc::new(Semaphore::new(product_parallel));
    let allocations = allocate_product_requests(&config);
    let mut tasks = FuturesUnordered::new();
    for (product, product_requests) in allocations {
        let permit = Arc::clone(&gate);
        let cfg = Arc::clone(&config);
        let pool = Arc::clone(&pool);
        let collector = Arc::clone(&collector);
        tasks.push(tokio::spawn(async move {
            let _permit = permit
                .acquire_owned()
                .await
                .map_err(|e| anyhow!("product parallel semaphore closed: {e}"))?;
            run_product(&cfg, &pool, &product, product_requests, &collector).await
        }));
    }
    let mut overall: Vec<ProductSummary> = Vec::with_capacity(config.products.len());
    while let Some(join_result) = tasks.next().await {
        let summary = join_result.map_err(|e| anyhow!("product task join: {e}"))??;
        overall.push(summary);
    }
    let order: HashMap<&str, usize> = config
        .products
        .iter()
        .enumerate()
        .map(|(i, p)| (p.product_id.as_str(), i))
        .collect();
    overall.sort_by_key(|s| {
        order
            .get(s.product_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    Ok(overall)
}

fn print_human_summary(
    overall: &[ProductSummary],
    total_requests: u32,
    total_success: u32,
    total_failure: u32,
    elapsed: f64,
) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "\n======== loadgen summary ========")?;
    writeln!(
        out,
        "total: requests={total_requests} success={total_success} failure={total_failure} elapsed_s={elapsed:.2}"
    )?;
    for summary in overall {
        let p50 = percentile(&summary.latencies_ms, 50.0).unwrap_or(0);
        let p95 = percentile(&summary.latencies_ms, 95.0).unwrap_or(0);
        let p99 = percentile(&summary.latencies_ms, 99.0).unwrap_or(0);
        let mut status_parts: Vec<String> = summary
            .status_counts
            .iter()
            .map(|(code, n)| format!("{code}:{n}"))
            .collect();
        status_parts.sort();
        let mut reason_parts: Vec<String> = summary
            .reason_counts
            .iter()
            .map(|(reason, n)| format!("{reason}:{n}"))
            .collect();
        reason_parts.sort();
        writeln!(
            out,
            "  - {} ({}) requests={} success={} failure={} p50={}ms p95={}ms p99={}ms",
            summary.name,
            summary.product_id,
            summary.requests,
            summary.success,
            summary.failure,
            p50,
            p95,
            p99
        )?;
        if !status_parts.is_empty() {
            writeln!(out, "      http_status=[{}]", status_parts.join(", "))?;
        }
        if !reason_parts.is_empty() {
            writeln!(out, "      reasons=[{}]", reason_parts.join(", "))?;
        }
        info!(
            name = %summary.name,
            product_id = %summary.product_id,
            requests = summary.requests,
            success = summary.success,
            failure = summary.failure,
            p50_ms = p50,
            p95_ms = p95,
            p99_ms = p99,
            status_counts = ?summary.status_counts,
            reason_counts = ?summary.reason_counts,
            "product summary"
        );
    }
    writeln!(out, "=================================\n")?;
    out.flush()?;
    Ok(())
}

fn allocate_product_requests(config: &LoadgenConfig) -> Vec<(ProductConfig, u32)> {
    let products = &config.products;
    if products.is_empty() {
        return Vec::new();
    }
    if let Some(total) = config.traffic.requests {
        // Requests mode: split the total by weight; products that get no share are not run
        // (requests=0 means "loop until deadline" in duration mode and must not leak here).
        let weights: Vec<u32> = products.iter().map(|p| p.weight.max(1)).collect();
        products
            .iter()
            .cloned()
            .zip(split_by_weight(total, &weights))
            .filter(|(_, n)| *n > 0)
            .collect()
    } else {
        // Duration mode: each product loops until deadline; requests=0 signals that.
        products.iter().map(|p| (p.clone(), 0)).collect()
    }
}

/// Split `total` across `weights` with the largest-remainder method: shares sum to `total`
/// and differ from the exact weighted share by less than one.
fn split_by_weight(total: u32, weights: &[u32]) -> Vec<u32> {
    let total_weight: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    if total_weight == 0 {
        return vec![0; weights.len()];
    }
    let mut shares: Vec<u32> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(u64, usize)> = Vec::with_capacity(weights.len());
    for (i, &w) in weights.iter().enumerate() {
        let exact = u64::from(total) * u64::from(w);
        shares.push(crate::num::u64_to_u32(exact / total_weight));
        remainders.push((exact % total_weight, i));
    }
    let assigned: u32 = shares.iter().sum();
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, i) in remainders
        .iter()
        .take(usize::try_from(total - assigned).unwrap_or(0))
    {
        shares[i] += 1;
    }
    shares
}

async fn build_transport_pool(config: &LoadgenConfig) -> Result<TransportPool> {
    let mut entries = Vec::with_capacity(config.resolved_customers.len());
    let mut cumulative = 0u32;
    for customer in &config.resolved_customers {
        let transport = build_transport_for_customer(config, customer).await?;
        cumulative = cumulative.saturating_add(customer.weight.max(1));
        entries.push((cumulative, transport));
    }
    if cumulative == 0 {
        anyhow::bail!("transport pool total weight is 0");
    }
    Ok(TransportPool {
        entries,
        total_weight: cumulative,
    })
}

async fn build_transport_for_customer(
    config: &LoadgenConfig,
    customer: &ResolvedCustomer,
) -> Result<ProxyTransport> {
    let tenant = &customer.tenant;
    if tenant.is_encryption_none() {
        let http = build_http_client(config)?;
        let fetched = tenant.fetch_token(&http, &config.token_url()).await?;
        let cached = cached_from_fetched(&fetched, config.traffic.token_refresh_skew_secs);
        info!(
            customer = %customer.name,
            encryption_mode = %tenant.encryption_mode,
            expires_in = fetched.expires_in,
            "using plain customer transport (Bearer + JSON)"
        );
        return Ok(ProxyTransport::Plain(Arc::new(PlainTransport {
            customer_name: customer.name.clone(),
            http,
            signer: tenant.request_signer()?,
            token: Mutex::new(cached),
            invoke_url: config.invoke_url(),
            token_url: config.token_url(),
            tenant: tenant.clone(),
            token_refresh_skew_secs: config.traffic.token_refresh_skew_secs,
        })));
    }

    if !tenant.is_encryption_enabled() {
        return Err(anyhow!(
            "customer {}: unsupported encryption_mode={}",
            customer.name,
            tenant.encryption_mode
        ));
    }

    let transport = build_tenant_transport_profile(tenant)?;
    let client = build_encrypted_client(config, tenant, transport).await?;
    info!(
        customer = %customer.name,
        encryption_mode = %tenant.encryption_mode,
        suite = %tenant.suite,
        "using encrypted customer transport (mazbri-sdk Profile)"
    );
    Ok(ProxyTransport::Encrypted(Arc::new(EncryptedTransport {
        customer_name: customer.name.clone(),
        client,
    })))
}

fn build_tenant_transport_profile(tenant: &TenantConfig) -> Result<TenantTransportProfile> {
    let mode = if tenant.is_encryption_request() {
        TransportMode::Request
    } else if tenant.is_encryption_full() {
        TransportMode::Full
    } else {
        return Err(anyhow!(
            "build_tenant_transport_profile requires encryption_mode=request|full"
        ));
    };
    let parse_alg = |field: &str, raw: Option<&str>| -> Result<Option<AlgorithmId>> {
        raw.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|raw| {
                raw.parse()
                    .map_err(|_| anyhow!("unsupported tenant.{field}={raw}"))
            })
            .transpose()
    };
    let suite = Some(tenant.suite.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let mut profile = TenantTransportProfile::plaintext();
    profile.mode = mode;
    profile.suite = suite;
    profile.data_alg = parse_alg("data_alg", tenant.data_alg.as_deref())?;
    profile.mac_alg = parse_alg("mac_alg", tenant.mac_alg.as_deref())?;
    let (data_alg, mac_alg) = profile
        .effective_algs()
        .map_err(|error| anyhow!("invalid tenant suite/data_alg/mac_alg: {error}"))?;
    profile.data_alg = data_alg;
    profile.mac_alg = mac_alg;
    profile.key = Some(KeySelection::Latest {
        key_id: tenant.key_version.trim().to_owned(),
    });
    let suite_has_mac = profile.mac_alg.is_some();
    profile.require_signature = tenant.require_signature.unwrap_or(suite_has_mac);
    profile.allow_insecure = tenant.allow_insecure;
    Ok(profile)
}

async fn build_encrypted_client(
    config: &LoadgenConfig,
    tenant: &TenantConfig,
    transport: TenantTransportProfile,
) -> Result<MazbriClient> {
    let tenant_key = tenant.tenant_key_bytes()?;
    MazbriClient::new(ClientConfig {
        gateway_base_url: config.gateway_base_url(),
        tenant_id: tenant.tenant_id.clone(),
        api_key: tenant.api_key.clone(),
        tenant_key,
        key_version: tenant.key_version.clone(),
        transport,
        request_signer: tenant.request_signer()?,
        token_refresh_skew_secs: config.traffic.token_refresh_skew_secs,
    })
    .await
    .context("failed to initialize SDK client")
}

fn cached_from_fetched(fetched: &FetchedToken, skew_secs: u64) -> CachedToken {
    let valid_for = fetched.expires_in.saturating_sub(skew_secs).max(1);
    CachedToken {
        access_token: fetched.access_token.clone(),
        refresh_after: Instant::now() + Duration::from_secs(valid_for),
    }
}

impl PlainTransport {
    async fn access_token(&self) -> Result<String> {
        {
            let guard = self.token.lock().await;
            if Instant::now() < guard.refresh_after {
                return Ok(guard.access_token.clone());
            }
        }
        let fetched = self.tenant.fetch_token(&self.http, &self.token_url).await?;
        let cached = cached_from_fetched(&fetched, self.token_refresh_skew_secs);
        let token = cached.access_token.clone();
        *self.token.lock().await = cached;
        info!(
            customer = %self.customer_name,
            expires_in = fetched.expires_in,
            "refreshed access token"
        );
        Ok(token)
    }
}

struct ProductCounters {
    latencies: Vec<u128>,
    success: u32,
    failure: u32,
    expected_reject: u32,
    status_counts: HashMap<u16, u32>,
    reason_counts: HashMap<String, u32>,
}

async fn apply_join_result(
    config: &LoadgenConfig,
    product: &ProductConfig,
    collector: &Mutex<MetricsCollector>,
    join_result: Result<Result<RequestOutcome>, tokio::task::JoinError>,
    counters: &mut ProductCounters,
) {
    match join_result {
        Ok(Ok(outcome)) => {
            counters.latencies.push(outcome.latency_ms);
            *counters
                .status_counts
                .entry(outcome.http_status)
                .or_default() += 1;
            if let Some(reason) = outcome.reason.as_deref() {
                *counters
                    .reason_counts
                    .entry(reason.to_string())
                    .or_default() += 1;
            }
            let expected =
                !outcome.success && is_expected_reason(&config.report, outcome.reason.as_deref());
            if outcome.success {
                counters.success += 1;
            } else {
                counters.failure += 1;
                if expected {
                    counters.expected_reject += 1;
                }
            }
            collector
                .lock()
                .await
                .record(&outcome_sample(product, &outcome, expected));
            log_outcome(product, &outcome);
        }
        Ok(Err(error)) => {
            let msg = format!("{error:#}");
            if msg.contains("duration deadline reached") {
                return;
            }
            counters.failure += 1;
            collector
                .lock()
                .await
                .record(&failed_task_sample(product, "task_error", msg));
            error!(
                product_id = %product.product_id,
                error = ?error,
                "request task failed"
            );
        }
        Err(join_error) => {
            counters.failure += 1;
            collector.lock().await.record(&failed_task_sample(
                product,
                "join_error",
                join_error.to_string(),
            ));
            error!(
                product_id = %product.product_id,
                error = ?join_error,
                "request join failed"
            );
        }
    }
}

async fn run_product(
    config: &LoadgenConfig,
    pool: &TransportPool,
    product: &ProductConfig,
    requests: u32,
    collector: &Mutex<MetricsCollector>,
) -> Result<ProductSummary> {
    let effective_qps = effective_qps(config, product);
    let concurrency = config.traffic.concurrency.max(1);
    let mut counters = ProductCounters {
        latencies: Vec::with_capacity(usize::try_from(requests).unwrap_or(0)),
        success: 0,
        failure: 0,
        expected_reject: 0,
        status_counts: HashMap::new(),
        reason_counts: HashMap::new(),
    };
    let product_started_at = Instant::now();
    let image_pool = load_image_pool(product)?;

    log_batch_start(
        product,
        concurrency,
        effective_qps,
        image_pool.as_ref().map_or(0, ImagePool::len),
        requests,
    );

    let mut tasks = spawn_requests(
        config,
        pool,
        product,
        requests,
        image_pool.as_ref(),
        effective_qps,
    );

    while let Some(join_result) = tasks.next().await {
        apply_join_result(config, product, collector, join_result, &mut counters).await;
    }

    let elapsed = product_started_at.elapsed().as_secs_f64();
    info!(
        product_id = %product.product_id,
        requests,
        success = counters.success,
        failure = counters.failure,
        elapsed_seconds = elapsed,
        "finished product batch"
    );
    println!(
        "finished {} ({}) success={} failure={} elapsed_s={:.2}",
        product.name, product.product_id, counters.success, counters.failure, elapsed
    );

    let completed = counters.success + counters.failure;
    Ok(ProductSummary {
        name: product.name.clone(),
        product_id: product.product_id.clone(),
        requests: if requests == 0 { completed } else { requests },
        success: counters.success,
        failure: counters.failure,
        expected_reject: counters.expected_reject,
        latencies_ms: counters.latencies,
        status_counts: counters.status_counts,
        reason_counts: counters.reason_counts,
    })
}

fn request_should_log_detail(request_index: u32) -> bool {
    request_index < 3 || request_index.is_multiple_of(500)
}

/// Locate vendored `ECharts` next to the loadgen crate / binary.
fn report_echarts_path() -> PathBuf {
    let candidates = [
        PathBuf::from("assets/echarts.min.js"),
        PathBuf::from("x-loadgen-bin/x-loadgen/assets/echarts.min.js"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/echarts.min.js"),
    ];
    for p in candidates {
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/echarts.min.js")
}

fn effective_qps(config: &LoadgenConfig, product: &ProductConfig) -> Option<u32> {
    // closed_loop ignores global QPS pacing (fills concurrency); product cap still applies as soft hint.
    if config.traffic.model.eq_ignore_ascii_case("closed_loop") {
        return product.qps_limit;
    }
    match (config.traffic.qps, product.qps_limit) {
        (Some(global), Some(product_cap)) => Some(global.min(product_cap)),
        (Some(global), None) => Some(global),
        (None, Some(product_cap)) => Some(product_cap),
        (None, None) => None,
    }
}

fn log_batch_start(
    product: &ProductConfig,
    concurrency: usize,
    effective_qps: Option<u32>,
    image_pool_size: usize,
    requests: u32,
) {
    info!(
        product_id = %product.product_id,
        name = %product.name,
        requests_per_product = requests,
        qps = effective_qps.unwrap_or(0),
        unbounded_qps = effective_qps.is_none(),
        concurrency,
        image_pool_size,
        "starting product batch"
    );
    println!(
        "starting {} ({}) requests={} concurrency={} qps={}",
        product.name,
        product.product_id,
        requests,
        concurrency,
        effective_qps.map_or_else(|| "unbounded".to_string(), |v| v.to_string())
    );
}

fn spawn_requests(
    config: &LoadgenConfig,
    pool: &TransportPool,
    product: &ProductConfig,
    requests: u32,
    image_pool: Option<&ImagePool>,
    effective_qps: Option<u32>,
) -> FuturesUnordered<tokio::task::JoinHandle<Result<RequestOutcome>>> {
    let concurrency = config.traffic.concurrency.max(1);
    let start = Instant::now();
    let deadline = config
        .traffic
        .duration_secs
        .map(|secs| start + Duration::from_secs(secs));
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let tasks = FuturesUnordered::new();
    // Duration mode: emit until deadline using a high virtual request budget paced by QPS/concurrency.
    let emit_count = if requests == 0 {
        // Approximate upper bound: concurrency * duration * 2.
        let secs = crate::num::u64_to_u32(config.traffic.duration_secs.unwrap_or(1).max(1));
        let qps = effective_qps
            .unwrap_or(crate::num::usize_to_u32(concurrency).saturating_mul(10))
            .max(1);
        qps.saturating_mul(secs)
            .saturating_mul(2)
            .max(crate::num::usize_to_u32(concurrency))
    } else {
        requests
    };
    for request_index in 0..emit_count {
        let scheduled = if let Some(qps) = effective_qps {
            let ramp = config.traffic.ramp_secs;
            let target_qps = f64::from(qps.max(1));
            let when = if ramp == 0 {
                f64::from(request_index) / target_qps
            } else {
                // Approximate linear ramp from 10% to 100% QPS over ramp_secs.
                let mut t = 0.0f64;
                for _i in 0..request_index {
                    let frac = (t / crate::num::u64_as_f64(ramp)).clamp(0.0, 1.0);
                    let q = target_qps * (0.1 + 0.9 * frac);
                    t += 1.0 / q.max(0.1);
                }
                t
            };
            start + Duration::from_secs_f64(when)
        } else {
            Instant::now()
        };
        if let Some(deadline) = deadline
            && scheduled >= deadline
        {
            break;
        }
        let permit = Arc::clone(&semaphore);
        let transport = pool.pick(u64::from(request_index)).clone();
        let config = config.clone();
        let product = product.clone();
        let image_pool = image_pool.cloned();
        tasks.push(tokio::spawn(async move {
            sleep_until(tokio::time::Instant::from_std(scheduled)).await;
            if let Some(deadline) = deadline
                && Instant::now() >= deadline
            {
                return Err(anyhow!("duration deadline reached"));
            }
            let _guard = permit
                .acquire_owned()
                .await
                .map_err(|error| anyhow!("semaphore closed: {error}"))?;
            let request_timeout =
                Duration::from_secs(product.timeout_secs.unwrap_or(config.traffic.timeout_secs));
            timeout(
                request_timeout,
                send_one_request(
                    &transport,
                    &config,
                    &product,
                    request_index,
                    image_pool.as_ref(),
                ),
            )
            .await
            .map_err(|_| {
                anyhow!(
                    "request timed out after {}s for product {}",
                    request_timeout.as_secs(),
                    product.product_id
                )
            })?
        }));
    }
    tasks
}

fn outcome_sample(
    product: &ProductConfig,
    outcome: &RequestOutcome,
    expected: bool,
) -> RequestSample {
    RequestSample {
        product_id: product.product_id.clone(),
        product_name: product.name.clone(),
        request_index: outcome.request_index,
        start_unix_ms: outcome.start_unix_ms,
        end_unix_ms: outcome.end_unix_ms,
        latency_ms: outcome.latency_ms,
        http_status: outcome.http_status,
        success: outcome.success,
        expected,
        reason: outcome.reason.clone(),
        error_class: outcome.error_class.clone(),
        response_bytes: outcome.response_bytes,
        request_id: outcome.request_id.clone(),
        error: outcome.error.clone(),
    }
}

fn log_outcome(product: &ProductConfig, outcome: &RequestOutcome) {
    if !outcome.success || request_should_log_detail(outcome.request_index) {
        tracing::info!(
            product_id = %product.product_id,
            request_index = outcome.request_index,
            http_status = outcome.http_status,
            proxy_status = outcome.proxy_status.unwrap_or_default(),
            success = outcome.success,
            latency_ms = outcome.latency_ms,
            response_bytes = outcome.response_bytes,
            reason = outcome.reason.as_deref().unwrap_or(""),
            error = outcome.error.as_deref().unwrap_or(""),
            "request completed"
        );
    }
}

fn failed_task_sample(product: &ProductConfig, error_class: &str, error: String) -> RequestSample {
    let now = unix_now_ms();
    RequestSample {
        product_id: product.product_id.clone(),
        product_name: product.name.clone(),
        request_index: 0,
        start_unix_ms: now,
        end_unix_ms: now,
        latency_ms: 0,
        http_status: 0,
        success: false,
        expected: false,
        reason: None,
        error_class: Some(error_class.into()),
        response_bytes: 0,
        request_id: None,
        error: Some(error),
    }
}

async fn send_one_request(
    transport: &ProxyTransport,
    config: &LoadgenConfig,
    product: &ProductConfig,
    request_index: u32,
    image_pool: Option<&ImagePool>,
) -> Result<RequestOutcome> {
    let request_id = Uuid::new_v4().to_string();
    let body = build_gateway_request(product, image_pool, request_index)?;
    let mut headers = HeaderMap::new();
    if config.traffic.include_request_id {
        headers.insert(
            HeaderName::from_static("x-request-id"),
            HeaderValue::from_str(request_id.as_str())
                .context("failed to build x-request-id header")?,
        );
    }
    let start_unix_ms = unix_now_ms();
    let request_started_at = Instant::now();
    let (http_status, response_bytes, envelope) = match transport {
        ProxyTransport::Encrypted(enc) => {
            let envelope = enc
                .client
                .call_envelope_with_headers(&body, &headers)
                .await
                .with_context(|| {
                    format!(
                        "proxy request failed for product {} customer={}",
                        product.product_id, enc.customer_name
                    )
                })?;
            (
                envelope.http_status,
                envelope.response_bytes,
                envelope.response,
            )
        }
        ProxyTransport::Plain(plain) => {
            let access_token = plain.access_token().await?;
            let body_bytes =
                serde_json::to_vec(&body).context("failed to serialize plain proxy body")?;
            let mut request = plain
                .http
                .post(&plain.invoke_url)
                .bearer_auth(access_token)
                .headers(headers)
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            if let Some(signer) = plain.signer.as_ref() {
                let binding = signer
                    .sign("POST", GATEWAY_INVOKE_PATH, &body_bytes)
                    .map_err(|error| anyhow!("request binding sign failed: {error}"))?;
                for (name, value) in binding.pairs() {
                    request = request.header(name, value);
                }
            }
            let response = request.body(body_bytes).send().await.with_context(|| {
                format!(
                    "plain proxy request failed for product {}",
                    product.product_id
                )
            })?;
            let http_status = response.status().as_u16();
            let raw = response
                .bytes()
                .await
                .context("failed to read plain proxy response body")?;
            let response_bytes = raw.len();
            let reason = extract_reason_from_raw_json(&raw);
            let envelope: GatewayResponseEnvelope =
                serde_json::from_slice(&raw).with_context(|| {
                    format!(
                        "failed to parse plain gateway response (HTTP {http_status}): {}",
                        String::from_utf8_lossy(&raw[..raw.len().min(500)])
                    )
                })?;
            return Ok(finish_outcome(
                product,
                RequestMeta {
                    request_index,
                    request_id,
                    request_started_at,
                    start_unix_ms,
                },
                http_status,
                response_bytes,
                &envelope,
                reason,
            ));
        }
    };
    let reason = extract_reason_from_error_text(envelope.error.as_deref());
    Ok(finish_outcome(
        product,
        RequestMeta {
            request_index,
            request_id,
            request_started_at,
            start_unix_ms,
        },
        http_status,
        response_bytes,
        &envelope,
        reason,
    ))
}

struct RequestMeta {
    request_index: u32,
    request_id: String,
    request_started_at: Instant,
    start_unix_ms: u64,
}

fn finish_outcome(
    product: &ProductConfig,
    meta: RequestMeta,
    http_status: u16,
    response_bytes: usize,
    envelope: &GatewayResponseEnvelope,
    reason: Option<String>,
) -> RequestOutcome {
    let RequestMeta {
        request_index,
        request_id,
        request_started_at,
        start_unix_ms,
    } = meta;
    let transport_ok = envelope.success && (200..300).contains(&http_status);
    let transport =
        TransportExpect::parse(&product.expect_transport).unwrap_or(TransportExpect::Ok);
    let judged = expect::judge(
        transport_ok,
        envelope.data.as_ref(),
        transport,
        &product.expect_checks,
    );
    let (success, error_class, error) = match judged {
        expect::Judge::Met => (true, None, None),
        expect::Judge::TransportFailed => {
            let class = if reason.is_some() {
                Some("rate_limit".into())
            } else {
                Some("gateway_error".into())
            };
            (false, class, envelope.error.clone())
        }
        expect::Judge::Failed { class, message } => (false, Some(class.into()), Some(message)),
    };
    RequestOutcome {
        request_index,
        http_status,
        proxy_status: Some(envelope.status),
        success,
        latency_ms: request_started_at.elapsed().as_millis(),
        response_bytes,
        error,
        reason,
        error_class,
        request_id: Some(request_id),
        start_unix_ms,
        end_unix_ms: unix_now_ms(),
    }
}

fn extract_reason_from_raw_json(raw: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(raw).ok()?;
    if let Some(reason) = value.get("reason").and_then(|v| v.as_str()) {
        let trimmed = reason.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    extract_reason_from_error_text(value.get("error").and_then(|v| v.as_str()))
}

fn extract_reason_from_error_text(error: Option<&str>) -> Option<String> {
    let err = error?;
    for key in [
        "tenant_qps",
        "api_qps",
        "client_ip_qps",
        "bandwidth",
        "deny_until",
        "deny_all",
        "snapshot_stale",
        "channel_qps",
        "queue_timeout",
        "queue_full",
    ] {
        if err.contains(key) {
            return Some(key.to_string());
        }
    }
    None
}

fn build_gateway_request(
    product: &ProductConfig,
    image_pool: Option<&ImagePool>,
    request_index: u32,
) -> Result<GatewayRequest> {
    let mut body = if let Some(raw) = product
        .body_json
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let rendered = template::render_body_template(raw, request_index)
            .context("failed to render product body template")?;
        serde_json::from_str::<Value>(&rendered).context("failed to parse product body_json")?
    } else {
        Value::Object(serde_json::Map::new())
    };

    if let Some(pool) = image_pool {
        let field = product
            .body_image_field
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("body_image_field is required when image pool is present"))?;
        let picked = pool.next_image(request_index)?;
        let name_field = product
            .body_image_name_field
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        match body {
            Value::Object(ref mut map) => {
                map.insert(field.to_string(), Value::String(picked.base64));
                if let Some(name_field) = name_field {
                    map.insert(name_field.to_string(), Value::String(picked.file_name));
                }
            }
            other => {
                return Err(anyhow!(
                    "product body must be a JSON object when body_image_field is set, got {other}"
                ));
            }
        }
    }

    let omit_empty_object = matches!(&body, Value::Object(map) if map.is_empty())
        && product.body_json.is_none()
        && image_pool.is_none();
    let body = if body == Value::Null || omit_empty_object {
        None
    } else {
        Some(body)
    };

    let headers = if product.headers.is_empty() {
        None
    } else {
        Some(
            product
                .headers
                .clone()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        )
    };
    let query = if product.query.is_empty() {
        None
    } else {
        Some(product.query.clone().into_iter().collect::<HashMap<_, _>>())
    };

    Ok(GatewayRequest {
        product_id: product.product_id.clone(),
        path: product.path.clone(),
        method: product.method.clone(),
        query,
        headers,
        body,
    })
}

#[derive(Debug, Clone)]
struct PickedImage {
    file_name: String,
    base64: String,
}

#[derive(Debug, Clone)]
struct ImagePool {
    files: Vec<PathBuf>,
    counter: Arc<AtomicUsize>,
}

impl ImagePool {
    fn next_image(&self, request_index: u32) -> Result<PickedImage> {
        if self.files.is_empty() {
            return Err(anyhow!("image pool is empty"));
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed);
        let path = &self.files[(idx + request_index as usize) % self.files.len()];
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("image file name is not utf-8: {}", path.display()))?
            .to_string();
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read image file {}", path.display()))?;
        Ok(PickedImage {
            file_name,
            base64: STANDARD.encode(bytes),
        })
    }

    fn len(&self) -> usize {
        self.files.len()
    }
}

fn load_image_pool(product: &ProductConfig) -> Result<Option<ImagePool>> {
    let Some(dir) = product
        .body_image_dir
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    let path = Path::new(dir);
    if !path.is_dir() {
        return Err(anyhow!(
            "body_image_dir is not a directory for product {}: {dir}",
            product.product_id
        ));
    }
    let mut files = fs::read_dir(path)
        .with_context(|| format!("failed to read body_image_dir {dir}"))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
                        )
                    })
        })
        .collect::<Vec<_>>();
    files.sort();
    if files.is_empty() {
        return Err(anyhow!(
            "no image files found in body_image_dir for product {}: {dir}",
            product.product_id
        ));
    }
    Ok(Some(ImagePool {
        files,
        counter: Arc::new(AtomicUsize::new(0)),
    }))
}

fn percentile(values: &[u128], p: f64) -> Option<u128> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = crate::num::f64_to_usize(
        ((p / 100.0) * (mazbri_core::num::usize_to_f64(sorted.len()) - 1.0)).round(),
    );
    sorted.get(rank).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Write;

    fn product_with_image(dir: &Path, name_field: Option<&str>) -> ProductConfig {
        ProductConfig {
            name: "img".into(),
            product_id: "p".into(),
            channel_code: None,
            path: "/imgdef".into(),
            method: "POST".into(),
            weight: 1,
            qps_limit: None,
            timeout_secs: None,
            query: BTreeMap::default(),
            headers: BTreeMap::default(),
            body_json: Some(r#"{"threshold":10}"#.into()),
            body_file: None,
            body_image_dir: Some(dir.display().to_string()),
            body_image_field: Some("image".into()),
            body_image_name_field: name_field.map(str::to_string),
            expect_transport: "ok".into(),
            expect_checks: Vec::new(),
        }
    }

    fn transport_tenant(tenant_extra: &str) -> TenantConfig {
        toml::from_str(&format!(
            "api_key = \"k\"\nencryption_mode = \"request\"\ntenant_id = \"t\"\n\
             tenant_key_base64 = \"AA==\"\nkey_version = \"v1\"\n{tenant_extra}\n"
        ))
        .unwrap()
    }

    #[test]
    fn transport_profile_supports_suite_custom_algs_and_rejects_conflicts() {
        let suite =
            build_tenant_transport_profile(&transport_tenant("suite = \"suite/gm-sm4-hmac-sm3\""))
                .unwrap();
        assert_eq!(suite.data_alg, Some(AlgorithmId::Sm4CbcPkcs7));
        assert_eq!(suite.mac_alg, Some(AlgorithmId::HmacSm3));
        assert!(suite.require_signature);

        // 自定义组合租户（mgmt 不落 suite）：只给 data_alg / mac_alg。
        let custom = build_tenant_transport_profile(&transport_tenant(
            "data_alg = \"aes-128-ctr\"\nmac_alg = \"hmac-sm3\"",
        ))
        .unwrap();
        assert_eq!(custom.suite, None);
        assert_eq!(custom.data_alg, Some(AlgorithmId::Aes128Ctr));
        assert_eq!(custom.mac_alg, Some(AlgorithmId::HmacSm3));
        assert!(custom.require_signature);

        let aead = build_tenant_transport_profile(&transport_tenant("data_alg = \"aes-192-gcm\""))
            .unwrap();
        assert_eq!(aead.mac_alg, None);
        assert!(!aead.require_signature);

        // suite 与显式算法冲突 → 报错；都不给 → 配置校验报错。
        assert!(
            build_tenant_transport_profile(&transport_tenant(
                "suite = \"suite/tenant-default-2026-11\"\ndata_alg = \"aes-128-gcm\"",
            ))
            .is_err()
        );
        let err = transport_tenant("")
            .validate("test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("suite or data_alg"), "{err}");
    }

    #[test]
    fn image_pick_writes_file_name_into_body() {
        let dir = std::env::temp_dir().join(format!("loadgen-img-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img_003.png");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"png-bytes").unwrap();
        let product = product_with_image(&dir, Some("filename"));
        let pool = load_image_pool(&product).unwrap().unwrap();
        let request = build_gateway_request(&product, Some(&pool), 0).unwrap();
        let body = request.body.unwrap();
        assert_eq!(body.get("filename"), Some(&json!("img_003.png")));
        assert_eq!(
            body.get("image").and_then(|v| v.as_str()),
            Some(STANDARD.encode(b"png-bytes").as_str())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_by_weight_sums_to_total_and_spreads_small_totals() {
        // 总请求数少于产品数：只有前 total 个产品各分到 1，其余为 0（不运行），总数不变。
        let small = split_by_weight(17, &[1; 34]);
        assert_eq!(small.iter().sum::<u32>(), 17);
        assert!(small.iter().all(|&n| n <= 1), "{small:?}");
        assert_eq!(split_by_weight(102, &[1; 34]), vec![3; 34]);
        assert_eq!(split_by_weight(10, &[1, 2, 2]), vec![2, 4, 4]);
        assert_eq!(split_by_weight(7, &[1, 1, 1]), vec![3, 2, 2]);
        assert_eq!(split_by_weight(0, &[1, 1]), vec![0, 0]);
    }

    #[test]
    fn mix_request_seed_spreads_equal_weights() {
        let total_weight = 3_u64;
        let mut counts = [0_u32; 3];
        for i in 0..300_u64 {
            let bucket = mix_request_seed(i) % total_weight;
            counts[usize::try_from(bucket).unwrap()] += 1;
        }
        // Rough balance: each bucket gets some share (not collapsed to one).
        assert!(counts.iter().all(|&c| c > 50), "counts={counts:?}");
        let max = *counts.iter().max().unwrap();
        let min = *counts.iter().min().unwrap();
        assert!(max - min < 80, "counts={counts:?}");
    }
}
