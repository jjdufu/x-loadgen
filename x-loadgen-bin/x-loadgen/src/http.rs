//! Shared HTTP client construction (timeouts and optional mTLS).

use anyhow::{Context, Result};
use reqwest::Client;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Options used to build a [`reqwest::Client`].
#[derive(Debug, Clone, Default)]
pub struct HttpClientOptions {
    /// TCP connect timeout in seconds.
    pub connect_timeout_secs: u64,
    /// Overall request timeout in seconds.
    pub timeout_secs: u64,
    /// `User-Agent` header value. Empty uses the reqwest default.
    pub user_agent: String,
    /// When true, disable keep-alive reuse (`pool_max_idle_per_host = 0`).
    pub disable_connection_pool: bool,
    /// Optional idle timeout for pooled connections, in seconds.
    pub pool_idle_timeout_secs: Option<u64>,
    /// PEM client certificate path for mTLS.
    pub client_cert: Option<String>,
    /// PEM client key path for mTLS (used together with [`Self::client_cert`]).
    pub client_key: Option<String>,
    /// Additional PEM CA certificate path.
    pub ca_cert: Option<String>,
    /// When true, accept invalid TLS certificates.
    pub insecure_skip_verify: bool,
}

impl HttpClientOptions {
    /// Defaults suitable for talking to a nearby gateway: 10s connect, 30s request.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connect_timeout_secs: 10,
            timeout_secs: 30,
            user_agent: format!("x-loadgen/{}", env!("CARGO_PKG_VERSION")),
            ..Self::default()
        }
    }
}

/// Build a `reqwest` client from [`HttpClientOptions`].
///
/// # Errors
///
/// Returns an error if certificate files cannot be read or the client cannot
/// be constructed.
pub fn build_http_client(options: &HttpClientOptions) -> Result<Client> {
    let connect = Duration::from_secs(options.connect_timeout_secs);
    let timeout = Duration::from_secs(options.timeout_secs);
    let mut builder = Client::builder()
        .connect_timeout(connect)
        .timeout(timeout)
        .tcp_nodelay(true);
    if !options.user_agent.is_empty() {
        builder = builder.user_agent(options.user_agent.clone());
    }
    if options.disable_connection_pool {
        builder = builder.pool_max_idle_per_host(0);
    }
    if let Some(secs) = options.pool_idle_timeout_secs {
        builder = builder.pool_idle_timeout(Duration::from_secs(secs));
    }
    let cert = options
        .client_cert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let key = options
        .client_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let (Some(cert_path), Some(key_path)) = (cert, key) {
        let cert_pem = fs::read(Path::new(cert_path))
            .with_context(|| format!("read client certificate {cert_path}"))?;
        let key_pem =
            fs::read(Path::new(key_path)).with_context(|| format!("read client key {key_path}"))?;
        let mut identity_pem = cert_pem;
        identity_pem.extend_from_slice(&key_pem);
        let identity = reqwest::Identity::from_pem(&identity_pem)
            .context("failed to load mTLS client identity (PEM cert+key)")?;
        builder = builder.identity(identity);
    }
    if let Some(ca) = options
        .ca_cert
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let pem = fs::read(Path::new(ca)).with_context(|| format!("read CA certificate {ca}"))?;
        let certificate =
            reqwest::Certificate::from_pem(&pem).context("failed to parse CA certificate PEM")?;
        builder = builder.add_root_certificate(certificate);
    }
    if options.insecure_skip_verify {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder.build().context("failed to build HTTP client")
}

#[cfg(test)]
mod tests {
    use super::{HttpClientOptions, build_http_client};

    #[test]
    fn builds_default_client() {
        let client = build_http_client(&HttpClientOptions::new());
        assert!(client.is_ok(), "default client: {client:?}");
    }
}
