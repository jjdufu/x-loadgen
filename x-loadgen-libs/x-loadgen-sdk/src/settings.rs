//! 页面录入的租户字段 → 传输 Profile / 网关会话。
//!
//! 规则与 mazbri v0.7.0 网关一致：
//! - `encryption_mode` 只能是 `none` / `request` / `full`；
//! - `request` / `full` 必须给出命名套件或 `data_alg`，以及租户 ID 与租户主密钥；
//! - `suite` 与显式 `data_alg` / `mac_alg` 同时出现时必须一致；
//! - legacy 算法（ECB / DES / HMAC-MD5）只在 `allow_insecure` 时放行；
//! - `require_signature` 缺省随生效 `mac_alg` 是否存在。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use x_loadgen_core::crypto::{
    AlgorithmId, TenantTransportProfile, TransportMode, known_suites, parse_algorithm_id,
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
    /// 租户唯一主密钥（32 字节标准 Base64）。
    pub tenant_key_base64: Option<&'a str>,
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
}

impl std::fmt::Debug for TenantSettings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantSettings")
            .field("encryption_mode", &self.encryption_mode)
            .field("tenant_id", &self.tenant_id)
            .field("suite", &self.suite)
            .field("data_alg", &self.data_alg)
            .field("mac_alg", &self.mac_alg)
            .field("require_signature", &self.require_signature)
            .field("allow_insecure", &self.allow_insecure)
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
    let mut profile = TenantTransportProfile::plaintext();
    profile.mode = mode.transport();
    profile.suite = suite.map(str::to_owned);
    profile.data_alg = data_alg;
    profile.mac_alg = mac_alg;
    let (data, mac) = profile.effective_algs().map_err(map_profile_error)?;
    profile.data_alg = data;
    profile.mac_alg = mac;
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

impl TenantSettings<'_> {
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
            transport,
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
            suite: Some("suite/aes-256-gcm"),
            tenant_key_base64: None,
            ..encrypted("full")
        };
        assert!(missing_key.session().is_err());
        let bad_key = TenantSettings {
            suite: Some("suite/aes-256-gcm"),
            tenant_key_base64: Some("not base64!"),
            ..encrypted("full")
        };
        assert!(bad_key.session().is_err());
        let short_key = TenantSettings {
            suite: Some("suite/aes-256-gcm"),
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
                suite: Some("suite/aes-256-gcm"),
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
                suite: Some("suite/aes-256-gcm"),
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
}
