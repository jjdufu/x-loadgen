//! 页面录入的租户字段 → 传输 Profile / 请求绑定签名器 / 网关会话。
//!
//! 规则与 mazbri 网关一致：
//! - `encryption_mode` 只能是 `none` / `request` / `full`；
//! - `request` / `full` 必须给出命名套件或 `data_alg`，以及租户 ID、密钥版本与租户密钥；
//! - `suite` 与显式 `data_alg` / `mac_alg` 同时出现时必须一致；
//! - legacy 算法（ECB / DES / HMAC-MD5）只在 `allow_insecure` 时放行；
//! - `require_signature` 缺省随生效 `mac_alg` 是否存在；
//! - 请求绑定 `kid` / `alg` / 密钥三者同时出现，`alg` 在闭集内：
//!   HMAC 密钥为标准 Base64，签名私钥为 PEM。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use x_loadgen_core::crypto::request_binding::{RequestSigner, parse_binding_alg};
use x_loadgen_core::crypto::{
    AlgorithmId, AlgorithmKind, KeySelection, TenantTransportProfile, TransportMode,
    generate_sign_keypair, known_suites, parse_algorithm_id,
};

use crate::session::{Error, GatewaySession, SessionConfig, map_profile_error};

/// 传输加密模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    /// 明文。
    None,
    /// 仅请求加密。
    Request,
    /// 请求与响应都加密。
    Full,
}

impl EncryptionMode {
    /// 解析 `none` / `request` / `full`。
    ///
    /// # Errors
    ///
    /// 其他取值返回 [`Error::Settings`]。
    pub fn parse(raw: &str) -> Result<Self, Error> {
        match raw.trim() {
            "none" => Ok(Self::None),
            "request" => Ok(Self::Request),
            "full" => Ok(Self::Full),
            other => Err(Error::Settings(format!(
                "encryption_mode must be none, request, or full (got {other:?})"
            ))),
        }
    }

    const fn transport(self) -> TransportMode {
        match self {
            Self::None => TransportMode::None,
            Self::Request => TransportMode::Request,
            Self::Full => TransportMode::Full,
        }
    }
}

/// 页面 / 数据库中的租户字段（均为可选字符串，空白视为未填）。
#[derive(Clone, Default)]
pub struct TenantSettings<'a> {
    /// `none` / `request` / `full`。
    pub encryption_mode: &'a str,
    /// 租户 UUID。
    pub tenant_id: Option<&'a str>,
    /// 租户密钥（标准 Base64）。
    pub tenant_key_base64: Option<&'a str>,
    /// 密钥版本。
    pub key_version: Option<&'a str>,
    /// 命名套件。
    pub suite: Option<&'a str>,
    /// 自定义数据算法。
    pub data_alg: Option<&'a str>,
    /// 自定义 MAC 算法。
    pub mac_alg: Option<&'a str>,
    /// 是否要求信封签名；缺省随 `mac_alg`。
    pub require_signature: Option<bool>,
    /// 是否放行 legacy 算法。
    pub allow_insecure: bool,
    /// 请求绑定 kid。
    pub binding_kid: Option<&'a str>,
    /// 请求绑定算法。
    pub binding_alg: Option<&'a str>,
    /// 请求绑定密钥（HMAC Base64 或私钥 PEM）。
    pub binding_key: Option<&'a str>,
}

impl std::fmt::Debug for TenantSettings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantSettings")
            .field("encryption_mode", &self.encryption_mode)
            .field("tenant_id", &self.tenant_id)
            .field("key_version", &self.key_version)
            .field("suite", &self.suite)
            .field("data_alg", &self.data_alg)
            .field("mac_alg", &self.mac_alg)
            .field("require_signature", &self.require_signature)
            .field("allow_insecure", &self.allow_insecure)
            .field("binding_kid", &self.binding_kid)
            .field("binding_alg", &self.binding_alg)
            .finish_non_exhaustive()
    }
}

fn filled(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn parse_alg(field: &str, raw: Option<&str>) -> Result<Option<AlgorithmId>, Error> {
    filled(raw)
        .map(|text| {
            parse_algorithm_id(text)
                .map_err(|_| Error::Settings(format!("unsupported {field} {text:?}")))
        })
        .transpose()
}

/// 由租户字段构造传输 Profile（已按注册表校验）。
///
/// # Errors
///
/// 模式非法、套件未知、算法不在闭集、组合冲突或 legacy 未放行时返回错误。
pub fn transport_profile(settings: &TenantSettings<'_>) -> Result<TenantTransportProfile, Error> {
    let mode = EncryptionMode::parse(settings.encryption_mode)?;
    let suite = filled(settings.suite);
    let data_alg = parse_alg("data_alg", settings.data_alg)?;
    let mac_alg = parse_alg("mac_alg", settings.mac_alg)?;
    if mode == EncryptionMode::None {
        if suite.is_some() || data_alg.is_some() || mac_alg.is_some() {
            return Err(Error::Settings(
                "encryption_mode none must not set suite, data_alg, or mac_alg".into(),
            ));
        }
        if settings.require_signature == Some(true) {
            return Err(Error::Settings(
                "encryption_mode none cannot require an envelope signature".into(),
            ));
        }
        return Ok(TenantTransportProfile::plaintext());
    }
    if let Some(name) = suite
        && !known_suites().contains(&name)
    {
        return Err(Error::Settings(format!(
            "unknown suite {name:?}; expected one of {}",
            known_suites().join(", ")
        )));
    }
    if suite.is_none() && data_alg.is_none() {
        return Err(Error::Settings(
            "encryption_mode request/full requires a suite or data_alg".into(),
        ));
    }
    let key_version = filled(settings.key_version).ok_or_else(|| {
        Error::Settings("encryption_mode request/full requires key_version".into())
    })?;
    let mut profile = TenantTransportProfile::plaintext();
    profile.mode = mode.transport();
    profile.suite = suite.map(str::to_owned);
    profile.data_alg = data_alg;
    profile.mac_alg = mac_alg;
    let (data, mac) = profile.effective_algs().map_err(map_profile_error)?;
    profile.data_alg = data;
    profile.mac_alg = mac;
    profile.key = Some(KeySelection::Latest {
        key_id: key_version.to_owned(),
    });
    profile.require_signature = settings.require_signature.unwrap_or(mac.is_some());
    if profile.require_signature && mac.is_none() {
        return Err(Error::Settings(
            "require_signature needs a mac_alg (set one or pick a suite with a MAC)".into(),
        ));
    }
    profile.allow_insecure = settings.allow_insecure;
    profile
        .validate(&x_loadgen_core::crypto::AlgorithmRegistry::default_registry())
        .map_err(map_profile_error)?;
    Ok(profile)
}

/// 由 kid、算法与密钥文本构造请求绑定签名器（构造时自检一次签名）。
///
/// # Errors
///
/// 字段缺失、算法不在闭集、HMAC 密钥非 Base64 或私钥非 PEM / 无法解析时返回错误。
pub fn binding_signer(kid: &str, alg: &str, key_text: &str) -> Result<RequestSigner, Error> {
    let kid = kid.trim();
    if kid.is_empty() {
        return Err(Error::Settings("binding_kid is required".into()));
    }
    let alg = parse_binding_alg(alg.trim())
        .map_err(|_| Error::Settings(format!("unsupported binding_alg {:?}", alg.trim())))?;
    let key_text = key_text.trim();
    let material = if alg.kind() == AlgorithmKind::Mac {
        let bytes = STANDARD.decode(key_text).map_err(|_| {
            Error::Settings("binding_key for an HMAC algorithm must be standard base64".into())
        })?;
        if bytes.is_empty() {
            return Err(Error::Settings("binding_key must not be empty".into()));
        }
        bytes
    } else {
        if !key_text.starts_with("-----BEGIN") {
            return Err(Error::Settings(
                "binding_key for a signature algorithm must be a private key PEM".into(),
            ));
        }
        key_text.as_bytes().to_vec()
    };
    RequestSigner::new(kid, alg, &material)
        .map_err(|error| Error::Settings(format!("binding_key rejected: {error}")))
}

/// 为非对称请求绑定算法生成密钥对，返回 `(私钥 PKCS#8 PEM, 公钥 SPKI PEM)`。
///
/// # Errors
///
/// 算法不在闭集或为 HMAC（HMAC 密钥由 mgmt 签发）时返回错误。
pub fn generate_binding_keypair(alg: &str) -> Result<(String, String), Error> {
    let id = parse_binding_alg(alg.trim())
        .map_err(|_| Error::Settings(format!("unsupported binding_alg {:?}", alg.trim())))?;
    if id.kind() == AlgorithmKind::Mac {
        return Err(Error::Settings(
            "HMAC binding keys are issued by mgmt; only signature algorithms have key pairs".into(),
        ));
    }
    let pair = generate_sign_keypair(id)
        .map_err(|error| Error::Settings(format!("key generation failed: {error}")))?;
    Ok((pair.private_pem.to_string(), pair.public_pem))
}

impl TenantSettings<'_> {
    /// 请求绑定签名器；三项都未填时为 `None`。
    ///
    /// # Errors
    ///
    /// 只填了部分字段，或字段非法时返回错误。
    pub fn signer(&self) -> Result<Option<RequestSigner>, Error> {
        match (
            filled(self.binding_kid),
            filled(self.binding_alg),
            filled(self.binding_key),
        ) {
            (None, None, None) => Ok(None),
            (Some(kid), Some(alg), Some(key)) => binding_signer(kid, alg, key).map(Some),
            _ => Err(Error::Settings(
                "request binding needs binding_kid, binding_alg, and binding_key together".into(),
            )),
        }
    }

    /// 构造网关会话，并用一次本地密封自检密钥长度与算法组合。
    ///
    /// # Errors
    ///
    /// 任一字段非法时返回 [`Error::Settings`] 等错误。
    pub fn session(&self) -> Result<GatewaySession, Error> {
        let transport = transport_profile(self)?;
        let encrypted = transport.mode != TransportMode::None;
        let (tenant_id, tenant_key) = if encrypted {
            let tenant_id = filled(self.tenant_id).ok_or_else(|| {
                Error::Settings("encryption_mode request/full requires tenant_id".into())
            })?;
            let key_text = filled(self.tenant_key_base64).ok_or_else(|| {
                Error::Settings("encryption_mode request/full requires tenant_key_base64".into())
            })?;
            let key = STANDARD
                .decode(key_text)
                .map_err(|_| Error::Settings("tenant_key_base64 must be standard base64".into()))?;
            (tenant_id.to_owned(), key)
        } else {
            (
                filled(self.tenant_id).unwrap_or_default().to_owned(),
                Vec::new(),
            )
        };
        let session = GatewaySession::new(SessionConfig {
            tenant_id,
            tenant_key,
            key_version: filled(self.key_version).unwrap_or_default().to_owned(),
            transport,
            request_signer: self.signer()?,
        })?;
        if encrypted {
            session
                .encode_request(b"{}", 1)
                .map_err(|error| Error::Settings(format!("tenant key check failed: {error}")))?;
        }
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY32: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    fn encrypted(mode: &str) -> TenantSettings<'_> {
        TenantSettings {
            encryption_mode: mode,
            tenant_id: Some("00000000-0000-0000-0000-000000000001"),
            tenant_key_base64: Some(KEY32),
            key_version: Some("v1"),
            ..TenantSettings::default()
        }
    }

    #[test]
    fn named_suites_and_custom_algs_build_sessions() {
        for suite in known_suites() {
            for mode in ["request", "full"] {
                let settings = TenantSettings {
                    suite: Some(suite),
                    ..encrypted(mode)
                };
                settings
                    .session()
                    .unwrap_or_else(|e| panic!("{suite}/{mode}: {e}"));
            }
        }
        let custom = TenantSettings {
            data_alg: Some("aes-256-cbc"),
            mac_alg: Some("hmac-sha256"),
            ..encrypted("full")
        };
        let profile = transport_profile(&custom).expect("custom");
        assert!(profile.require_signature);
        custom.session().expect("custom session");
    }

    #[test]
    fn request_and_full_need_suite_or_data_alg_and_credentials() {
        assert!(transport_profile(&encrypted("full")).is_err());
        let missing_key = TenantSettings {
            suite: Some("suite/tenant-default-2026-11"),
            tenant_key_base64: None,
            ..encrypted("full")
        };
        assert!(missing_key.session().is_err());
        let bad_key = TenantSettings {
            suite: Some("suite/tenant-default-2026-11"),
            tenant_key_base64: Some("not base64!"),
            ..encrypted("full")
        };
        assert!(bad_key.session().is_err());
        let short_key = TenantSettings {
            suite: Some("suite/tenant-default-2026-11"),
            tenant_key_base64: Some("AAEC"),
            ..encrypted("full")
        };
        assert!(short_key.session().is_err());
        assert!(
            TenantSettings {
                suite: Some("suite/unknown"),
                ..encrypted("full")
            }
            .session()
            .is_err()
        );
        assert!(EncryptionMode::parse("FULL").is_err());
    }

    #[test]
    fn legacy_needs_allow_insecure_and_cbc_needs_mac() {
        let legacy = TenantSettings {
            data_alg: Some("aes-256-ecb"),
            mac_alg: Some("hmac-md5"),
            ..encrypted("full")
        };
        assert!(matches!(legacy.session(), Err(Error::LegacyDenied(_))));
        TenantSettings {
            allow_insecure: true,
            ..legacy
        }
        .session()
        .expect("legacy allowed");
        assert!(
            TenantSettings {
                data_alg: Some("aes-256-cbc"),
                ..encrypted("full")
            }
            .session()
            .is_err()
        );
        assert!(
            TenantSettings {
                suite: Some("suite/tenant-default-2026-11"),
                data_alg: Some("sm4-cbc-pkcs7"),
                ..encrypted("full")
            }
            .session()
            .is_err()
        );
    }

    #[test]
    fn none_mode_rejects_crypto_fields() {
        TenantSettings {
            encryption_mode: "none",
            ..TenantSettings::default()
        }
        .session()
        .expect("plain");
        assert!(
            TenantSettings {
                encryption_mode: "none",
                suite: Some("suite/tenant-default-2026-11"),
                ..TenantSettings::default()
            }
            .session()
            .is_err()
        );
        assert!(
            TenantSettings {
                encryption_mode: "none",
                require_signature: Some(true),
                ..TenantSettings::default()
            }
            .session()
            .is_err()
        );
    }

    #[test]
    fn binding_fields_are_checked_together() {
        let partial = TenantSettings {
            encryption_mode: "none",
            binding_kid: Some("1"),
            ..TenantSettings::default()
        };
        assert!(partial.session().is_err());
        assert!(binding_signer("1", "hmac-md5", KEY32).is_err());
        assert!(binding_signer("1", "hmac-sha256", "%%%").is_err());
        binding_signer("1", "hmac-sm3", KEY32).expect("hmac-sm3");
        let (private_pem, public_pem) = generate_binding_keypair("ed25519").expect("pair");
        assert!(public_pem.starts_with("-----BEGIN PUBLIC KEY-----"));
        binding_signer("2", "ed25519", &private_pem).expect("ed25519 signer");
        assert!(binding_signer("2", "ed25519", KEY32).is_err());
        assert!(generate_binding_keypair("hmac-sha256").is_err());
        assert!(generate_binding_keypair("rsa-pss").is_err());
    }
}
