//! Shared Hoppscotch-style request, auth, TLS, variable, assertion, and Mazbri
//! tenant primitives persisted across collections, environments, and snapshots.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use x_loadgen_sdk::{TenantSettings, generate_binding_keypair};

use super::error::StoreError;
use super::unix_now_ms;
use crate::expect::{FieldCheck, TransportExpect};

/// Generic HTTP / REST protocol.
pub const PROTOCOL_HTTP: &str = "http";
/// Mazbri gateway protocol (token exchange + request binding + envelope crypto).
pub const PROTOCOL_MAZBRI: &str = "mazbri";

/// Generate a new `UUIDv4` string identifier.
#[must_use]
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Current Unix timestamp in milliseconds as `i64`.
///
/// # Errors
///
/// Returns an error if the system clock is before the Unix epoch.
pub fn now_ms() -> Result<i64, StoreError> {
    let ms = unix_now_ms().map_err(|err| StoreError::Other(err.into()))?;
    Ok(i64::try_from(ms).unwrap_or(i64::MAX))
}

const fn default_true() -> bool {
    true
}

const fn default_weight() -> i64 {
    1
}

fn default_protocol() -> String {
    PROTOCOL_HTTP.to_string()
}

fn default_auth_inherit() -> String {
    "inherit".to_string()
}

fn default_encryption_none() -> String {
    "none".to_string()
}

fn default_tenant_source() -> String {
    "environment".to_string()
}

fn default_expect_ok() -> String {
    "ok".to_string()
}

/// Validate protocol string (`"http"` or `"mazbri"`).
///
/// # Errors
///
/// Returns validation error when `protocol` is neither `"http"` nor `"mazbri"`.
pub fn validate_protocol(protocol: &str) -> Result<String, StoreError> {
    let trimmed = protocol.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        PROTOCOL_HTTP | PROTOCOL_MAZBRI => Ok(trimmed),
        _ => Err(StoreError::invalid(format!(
            "protocol must be 'http' or 'mazbri', got {protocol:?}"
        ))),
    }
}

/// Validate a base URL when non-empty (`http://` or `https://` with non-empty host).
///
/// # Errors
///
/// Returns validation error if the scheme or host is invalid.
pub fn validate_optional_base_url(
    url: Option<&str>,
    field: &str,
) -> Result<Option<String>, StoreError> {
    let Some(trimmed) = url.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    // Allow Hoppscotch template placeholders at the start (e.g. <<baseUrl>>).
    if trimmed.starts_with("<<") || trimmed.starts_with("{{") {
        return Ok(Some(trimmed.to_string()));
    }
    validate_http_url(trimmed, field)?;
    Ok(Some(trimmed.trim_end_matches('/').to_string()))
}

/// Validate an `http://` or `https://` URL.
///
/// # Errors
///
/// Returns validation error when missing scheme or host.
pub fn validate_http_url(url: &str, field: &str) -> Result<(), StoreError> {
    let trimmed = url.trim();
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .ok_or_else(|| {
            StoreError::invalid(format!("{field} must start with http:// or https://"))
        })?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.trim().is_empty() {
        return Err(StoreError::invalid(format!("{field} has an empty host")));
    }
    Ok(())
}

/// One Hoppscotch-style key/value row with an active checkbox and optional description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueRow {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One environment or collection variable row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVarRow {
    #[serde(default)]
    pub key: String,
    #[serde(default, alias = "currentValue", alias = "initialValue")]
    pub value: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default = "default_true")]
    pub active: bool,
}

impl EnvVarRow {
    /// Return a redacted copy where secret values are masked.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            key: self.key.clone(),
            value: if self.secret {
                String::new()
            } else {
                self.value.clone()
            },
            secret: self.secret,
            active: self.active,
        }
    }
}

/// Hoppscotch-compatible request body specification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestBodySpec {
    #[serde(default, alias = "contentType")]
    pub content_type: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub form: Vec<KeyValueRow>,
}

/// Hoppscotch-compatible HTTP authorization configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpAuthSpec {
    #[serde(default = "default_auth_inherit", alias = "authType")]
    pub auth_type: String,
    #[serde(default = "default_true", alias = "authActive")]
    pub auth_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, alias = "addTo", skip_serializing_if = "Option::is_none")]
    pub add_to: Option<String>,
}

impl Default for HttpAuthSpec {
    fn default() -> Self {
        Self {
            auth_type: default_auth_inherit(),
            auth_active: true,
            token: None,
            username: None,
            password: None,
            key: None,
            value: None,
            add_to: None,
        }
    }
}

impl HttpAuthSpec {
    /// Default `"none"` auth for root environments.
    #[must_use]
    pub fn none() -> Self {
        Self {
            auth_type: "none".to_string(),
            ..Self::default()
        }
    }

    /// Validate auth type.
    ///
    /// # Errors
    ///
    /// Returns validation error when `auth_type` or `add_to` is unsupported.
    pub fn validate(&self) -> Result<(), StoreError> {
        match self.auth_type.trim() {
            "inherit" | "none" | "bearer" | "basic" | "api-key" => {}
            other => {
                return Err(StoreError::invalid(format!(
                    "unsupported auth_type {other:?} (expected inherit, none, bearer, basic, or api-key)"
                )));
            }
        }
        if self.auth_type == "api-key"
            && let Some(add_to) = self.add_to.as_deref()
            && !matches!(add_to, "HEADERS" | "QUERY_PARAMS")
        {
            return Err(StoreError::invalid(format!(
                "api-key add_to must be HEADERS or QUERY_PARAMS, got {add_to:?}"
            )));
        }
        Ok(())
    }

    /// Return a redacted copy removing secret credentials.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            auth_type: self.auth_type.clone(),
            auth_active: self.auth_active,
            token: None,
            username: self.username.clone(),
            password: None,
            key: self.key.clone(),
            value: None,
            add_to: self.add_to.clone(),
        }
    }
}

/// TLS / mTLS configuration (supports inline PEM strings or server file paths).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_cert: Option<String>,
    #[serde(default)]
    pub insecure_skip_verify: bool,
}

impl TlsSpec {
    /// True if all TLS overrides are empty and `insecure_skip_verify` is false.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        non_empty_str(self.client_cert.as_deref()).is_none()
            && non_empty_str(self.client_key.as_deref()).is_none()
            && non_empty_str(self.ca_cert.as_deref()).is_none()
            && !self.insecure_skip_verify
    }

    /// Merge request-level TLS override on top of environment-level TLS config.
    #[must_use]
    pub fn overlay_on(&self, base: &Self) -> Self {
        if self.is_empty() {
            return base.clone();
        }
        Self {
            client_cert: non_empty_owned(self.client_cert.as_deref())
                .or_else(|| non_empty_owned(base.client_cert.as_deref())),
            client_key: non_empty_owned(self.client_key.as_deref())
                .or_else(|| non_empty_owned(base.client_key.as_deref())),
            ca_cert: non_empty_owned(self.ca_cert.as_deref())
                .or_else(|| non_empty_owned(base.ca_cert.as_deref())),
            insecure_skip_verify: self.insecure_skip_verify || base.insecure_skip_verify,
        }
    }

    /// Validate mTLS cert/key pairing.
    ///
    /// # Errors
    ///
    /// Returns validation error when only one of `client_cert` / `client_key` is provided.
    pub fn validate(&self) -> Result<(), StoreError> {
        let has_cert = non_empty_str(self.client_cert.as_deref()).is_some();
        let has_key = non_empty_str(self.client_key.as_deref()).is_some();
        if has_cert != has_key {
            return Err(StoreError::invalid(
                "client_cert and client_key must both be set when configuring mTLS",
            ));
        }
        Ok(())
    }

    /// Return a redacted copy without private key material.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            client_cert: self.client_cert.clone(),
            client_key: None,
            ca_cert: self.ca_cert.clone(),
            insecure_skip_verify: self.insecure_skip_verify,
        }
    }
}

/// Transport & JSON field assertions attached to a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionsSpec {
    #[serde(default = "default_expect_ok", alias = "expectTransport")]
    pub expect_transport: String,
    #[serde(default)]
    pub checks: Vec<FieldCheck>,
}

impl Default for AssertionsSpec {
    fn default() -> Self {
        Self {
            expect_transport: default_expect_ok(),
            checks: Vec::new(),
        }
    }
}

impl AssertionsSpec {
    /// Validate `expect_transport` (`"ok"` or `"reject"`).
    ///
    /// # Errors
    ///
    /// Returns validation error when `expect_transport` is invalid or a check path is empty.
    pub fn validate(&self) -> Result<(), StoreError> {
        if TransportExpect::parse(self.expect_transport.trim()).is_none() {
            return Err(StoreError::invalid(format!(
                "expect_transport must be 'ok' or 'reject', got {:?}",
                self.expect_transport
            )));
        }
        for check in &self.checks {
            if check.path.trim().is_empty() {
                return Err(StoreError::invalid(
                    "assertion check path must not be empty",
                ));
            }
        }
        Ok(())
    }
}

/// Mazbri tenant profile (either in an environment's weighted pool or inline on a request).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantSpec {
    #[serde(default = "new_id")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_encryption_none")]
    pub encryption_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_key_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_alg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_alg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_signature: Option<bool>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_kid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_alg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_public_key: Option<String>,
}

impl Default for TenantSpec {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "default".to_string(),
            weight: 1,
            api_key: None,
            encryption_mode: default_encryption_none(),
            tenant_id: None,
            tenant_key_base64: None,
            key_version: None,
            suite: None,
            data_alg: None,
            mac_alg: None,
            require_signature: None,
            allow_insecure: false,
            binding_kid: None,
            binding_alg: None,
            binding_key: None,
            binding_public_key: None,
        }
    }
}

impl TenantSpec {
    /// Convert into [`TenantSettings`] for `x-loadgen-sdk`.
    #[must_use]
    pub fn settings(&self) -> TenantSettings<'_> {
        TenantSettings {
            encryption_mode: self.encryption_mode.trim(),
            tenant_id: non_empty_str(self.tenant_id.as_deref()),
            tenant_key_base64: non_empty_str(self.tenant_key_base64.as_deref()),
            key_version: non_empty_str(self.key_version.as_deref()),
            suite: non_empty_str(self.suite.as_deref()),
            data_alg: non_empty_str(self.data_alg.as_deref()),
            mac_alg: non_empty_str(self.mac_alg.as_deref()),
            require_signature: self.require_signature,
            allow_insecure: self.allow_insecure,
            binding_kid: non_empty_str(self.binding_kid.as_deref()),
            binding_alg: non_empty_str(self.binding_alg.as_deref()),
            binding_key: non_empty_str(self.binding_key.as_deref()),
        }
    }

    /// Validate weight, `api_key`, and full cryptographic session construction
    /// (including a dry-run `encode_request` check).
    ///
    /// # Errors
    ///
    /// Returns validation error if `weight <= 0`, `api_key` is missing, or any
    /// crypto / signature parameter is invalid.
    pub fn validate_for_mazbri(&self) -> Result<(), StoreError> {
        if self.name.trim().is_empty() {
            return Err(StoreError::invalid("tenant name must not be empty"));
        }
        if self.weight <= 0 {
            return Err(StoreError::invalid("tenant weight must be > 0"));
        }
        if non_empty_str(self.api_key.as_deref()).is_none() {
            return Err(StoreError::invalid(format!(
                "mazbri tenant '{}' requires a non-empty api_key",
                self.name
            )));
        }
        let session = self
            .settings()
            .session()
            .map_err(|err| StoreError::invalid(format!("tenant '{}': {err}", self.name)))?;
        session
            .encode_request(b"{}", 1_700_000_000)
            .map_err(|err| StoreError::invalid(format!("tenant '{}': {err}", self.name)))?;
        Ok(())
    }

    /// Validate a tenant stored in an environment pool:
    /// if any Mazbri crypto/signature field is populated or `encryption_mode != "none"`,
    /// validates the full Mazbri session; otherwise validates basic weight/name so it
    /// can also serve as an HTTP multi-key tenant.
    ///
    /// # Errors
    ///
    /// Returns validation error when fields are inconsistent.
    pub fn validate_pool_entry(&self) -> Result<(), StoreError> {
        if self.name.trim().is_empty() {
            return Err(StoreError::invalid("tenant name must not be empty"));
        }
        if self.weight <= 0 {
            return Err(StoreError::invalid("tenant weight must be > 0"));
        }
        let has_mazbri_crypto = self.encryption_mode.trim() != "none"
            || non_empty_str(self.suite.as_deref()).is_some()
            || non_empty_str(self.data_alg.as_deref()).is_some()
            || non_empty_str(self.binding_alg.as_deref()).is_some()
            || non_empty_str(self.binding_kid.as_deref()).is_some()
            || non_empty_str(self.binding_key.as_deref()).is_some();
        if has_mazbri_crypto {
            self.validate_for_mazbri()?;
        }
        Ok(())
    }

    /// Strip secret keys (`api_key`, `tenant_key_base64`, `binding_key`) while
    /// keeping algorithm names and `binding_public_key`.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            api_key: None,
            tenant_key_base64: None,
            binding_key: None,
            ..self.clone()
        }
    }
}

/// Mazbri-specific request configuration attached when `protocol == "mazbri"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MazbriRequestSpec {
    #[serde(default)]
    pub product_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_base_url: Option<String>,
    #[serde(default = "default_tenant_source")]
    pub tenant_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_tenant: Option<TenantSpec>,
}

impl Default for MazbriRequestSpec {
    fn default() -> Self {
        Self {
            product_id: String::new(),
            gateway_base_url: None,
            tenant_source: default_tenant_source(),
            tenant_name: None,
            inline_tenant: None,
        }
    }
}

impl MazbriRequestSpec {
    /// Validate Mazbri fields on a saved or executed request.
    ///
    /// # Errors
    ///
    /// Returns validation error if `tenant_source` is invalid or `inline_tenant` is malformed.
    pub fn validate(&self) -> Result<(), StoreError> {
        if let Some(url) = self.gateway_base_url.as_deref() {
            let _ = validate_optional_base_url(Some(url), "mazbri.gateway_base_url")?;
        }
        match self.tenant_source.trim() {
            "environment" => {}
            "inline" => {
                let tenant = self.inline_tenant.as_ref().ok_or_else(|| {
                    StoreError::invalid(
                        "mazbri.inline_tenant is required when tenant_source is 'inline'",
                    )
                })?;
                tenant.validate_for_mazbri()?;
            }
            other => {
                return Err(StoreError::invalid(format!(
                    "mazbri.tenant_source must be 'environment' or 'inline', got {other:?}"
                )));
            }
        }
        Ok(())
    }

    /// Return a redacted copy with inline tenant secrets stripped.
    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            product_id: self.product_id.clone(),
            gateway_base_url: self.gateway_base_url.clone(),
            tenant_source: self.tenant_source.clone(),
            tenant_name: self.tenant_name.clone(),
            inline_tenant: self.inline_tenant.as_ref().map(TenantSpec::redacted),
        }
    }
}

/// Output of asymmetric request-binding keypair generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedBindingKeypair {
    pub binding_kid: String,
    pub binding_alg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_pem: Option<String>,
    pub public_key_pem: String,
}

/// Generate a fresh asymmetric request-binding keypair (`ed25519`, `sm2-sign`,
/// `ecdsa-p256-sha256`, `ecdsa-p384-sha384`, `rsa-pkcs1v15-sign-sha256/384/512`).
///
/// # Errors
///
/// Returns validation error if `alg` is unsupported/symmetric or `kid` is empty.
pub fn create_binding_keypair(
    alg: &str,
    kid: &str,
    include_private_key: bool,
) -> Result<(String, GeneratedBindingKeypair), StoreError> {
    let trimmed_alg = alg.trim();
    let trimmed_kid = kid.trim();
    if trimmed_kid.is_empty() {
        return Err(StoreError::invalid("binding_kid must not be empty"));
    }
    let (private_key_pem, public_key_pem) = generate_binding_keypair(trimmed_alg)
        .map_err(|err| StoreError::invalid(format!("generate keypair ({trimmed_alg}): {err}")))?;
    let private_pem = private_key_pem.clone();
    let out = GeneratedBindingKeypair {
        binding_kid: trimmed_kid.to_string(),
        binding_alg: trimmed_alg.to_string(),
        private_key_pem: include_private_key.then_some(private_key_pem),
        public_key_pem,
    };
    Ok((private_pem, out))
}

/// Helper returning `Some(trimmed)` when non-empty.
#[must_use]
pub fn non_empty_str(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Helper returning `Some(trimmed.to_string())` when non-empty.
#[must_use]
pub fn non_empty_owned(value: Option<&str>) -> Option<String> {
    non_empty_str(value).map(str::to_string)
}

/// Helper storing `PROTOCOL_HTTP` default in serde structs.
#[must_use]
pub fn default_http_protocol() -> String {
    default_protocol()
}
