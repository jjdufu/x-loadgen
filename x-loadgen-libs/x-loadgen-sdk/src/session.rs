//! 单租户网关会话：按 Profile 密封请求、附加绑定头、解码响应。
//!
//! 会话只持有只读密钥材料，`&self` 方法可并发调用；每次请求生成新的传输 nonce 与绑定 nonce。

use x_loadgen_core::crypto::request_binding::{RequestBindingError, RequestSigner};
use x_loadgen_core::crypto::transport::{
    SealEnvelopeParams, TransportCryptoError, TransportEnvelope, open_envelope, seal_envelope,
};
use x_loadgen_core::crypto::{
    AlgorithmRegistry, CryptoError, TenantTransportProfile, TransportMode,
};

use crate::contract::{GATEWAY_INVOKE_PATH, GatewayResponseEnvelope};

/// 客户端错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// JSON 编解码失败。
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// 信封密封 / 解封 / MAC 校验失败。
    #[error("crypto error: {0}")]
    Crypto(#[from] TransportCryptoError),
    /// 换 token 失败。
    #[error("token request failed: {0}")]
    Token(String),
    /// 网关响应与合同不符。
    #[error("gateway response invalid: {0}")]
    Gateway(String),
    /// 算法不在闭集或未登记。
    #[error("unsupported transport algorithm: {0}")]
    UnsupportedAlgorithm(String),
    /// legacy 算法未放行。
    #[error("legacy transport algorithm denied: {0}")]
    LegacyDenied(String),
    /// 请求绑定签名失败。
    #[error("request binding failed: {0}")]
    Binding(#[from] RequestBindingError),
    /// 页面录入的租户配置非法。
    #[error("invalid tenant settings: {0}")]
    Settings(String),
}

/// 会话配置。
#[derive(Clone)]
pub struct SessionConfig {
    /// 租户 UUID（也是 AEAD AAD 与 MAC 规范串的一部分）。
    pub tenant_id: String,
    /// 租户数据密钥（`mac_alg` 存在时也作 MAC 密钥）。
    pub tenant_key: Vec<u8>,
    /// 当前密钥版本（如 `v1`）。
    pub key_version: String,
    /// 租户传输 Profile。
    pub transport: TenantTransportProfile,
    /// 可选请求绑定签名器。
    pub request_signer: Option<RequestSigner>,
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("tenant_id", &self.tenant_id)
            .field("tenant_key", &"<redacted>")
            .field("key_version", &self.key_version)
            .field("transport", &self.transport)
            .field("request_signer", &self.request_signer)
            .finish()
    }
}

/// 编码后的网关请求：最终请求体与需要附加的请求头。
#[derive(Debug, Clone)]
pub struct EncodedRequest {
    /// 请求体（明文 JSON 或信封 JSON）。
    pub body: Vec<u8>,
    /// 绑定头（头名小写）；未配置签名器时为空。
    pub headers: Vec<(&'static str, String)>,
}

/// 单租户网关会话。
#[derive(Debug, Clone)]
pub struct GatewaySession {
    config: SessionConfig,
    registry: AlgorithmRegistry,
}

fn _assert_gateway_session_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GatewaySession>();
}

impl GatewaySession {
    /// 校验 Profile 并构造会话。
    ///
    /// # Errors
    ///
    /// Profile 非法（未知套件、legacy 未放行、缺数据算法等）时返回错误。
    pub fn new(config: SessionConfig) -> Result<Self, Error> {
        let registry = AlgorithmRegistry::default_registry();
        config
            .transport
            .validate(&registry)
            .map_err(map_profile_error)?;
        Ok(Self { config, registry })
    }

    /// 传输模式。
    #[must_use]
    pub fn mode(&self) -> TransportMode {
        self.config.transport.mode
    }

    /// 是否配置了请求绑定。
    #[must_use]
    pub fn has_binding(&self) -> bool {
        self.config.request_signer.is_some()
    }

    /// 编码一次网关调用（`plaintext` 为 [`crate::GatewayRequest`] JSON）。
    ///
    /// # Errors
    ///
    /// 密封或绑定签名失败时返回错误。
    pub fn encode_request(
        &self,
        plaintext: &[u8],
        timestamp: i64,
    ) -> Result<EncodedRequest, Error> {
        let body = match self.config.transport.mode {
            TransportMode::None => plaintext.to_vec(),
            TransportMode::Request | TransportMode::Full => {
                let envelope = self.seal_request(plaintext, timestamp)?;
                serde_json::to_vec(&envelope)?
            }
        };
        let headers = match self.config.request_signer.as_ref() {
            Some(signer) => signer
                .sign("POST", GATEWAY_INVOKE_PATH, &body)?
                .pairs()
                .into_iter()
                .collect(),
            None => Vec::new(),
        };
        Ok(EncodedRequest { body, headers })
    }

    /// 按传输模式解码网关响应体。
    ///
    /// # Errors
    ///
    /// 响应与模式不符、解封失败或 JSON 非法时返回错误。
    pub fn decode_response(
        &self,
        http_status: u16,
        raw: &[u8],
    ) -> Result<GatewayResponseEnvelope, Error> {
        decode_gateway_payload(self.config.transport.mode, http_status, raw, |envelope| {
            self.open_response(envelope)
        })
    }

    fn seal_request(&self, plaintext: &[u8], timestamp: i64) -> Result<TransportEnvelope, Error> {
        let (_, mac_alg) = self
            .config
            .transport
            .effective_algs()
            .map_err(map_profile_error)?;
        let mac_key = mac_alg.map(|_| self.config.tenant_key.as_slice());
        seal_envelope(SealEnvelopeParams {
            profile: &self.config.transport,
            registry: &self.registry,
            data_key: &self.config.tenant_key,
            mac_key,
            key_version: &self.config.key_version,
            tenant_id: &self.config.tenant_id,
            http_path: GATEWAY_INVOKE_PATH,
            plaintext,
            timestamp,
        })
        .map_err(map_crypto_error)
    }

    fn open_response(&self, envelope: &TransportEnvelope) -> Result<Vec<u8>, Error> {
        let mac_key = envelope
            .mac_alg
            .as_ref()
            .map(|_| self.config.tenant_key.as_slice());
        open_envelope(
            &self.config.transport,
            &self.registry,
            &self.config.tenant_key,
            mac_key,
            &self.config.tenant_id,
            GATEWAY_INVOKE_PATH,
            envelope,
        )
        .map_err(map_crypto_error)
    }
}

pub(crate) fn map_profile_error(error: CryptoError) -> Error {
    match error {
        CryptoError::LegacyNotAllowed { id } => Error::LegacyDenied(id.as_str().to_owned()),
        CryptoError::UnknownAlgorithmId { raw } => Error::UnsupportedAlgorithm(raw),
        CryptoError::NotRegistered { id } => Error::UnsupportedAlgorithm(id.as_str().to_owned()),
        CryptoError::Unsupported { reason } => Error::UnsupportedAlgorithm(reason),
        other => Error::Settings(format!("invalid transport profile: {other}")),
    }
}

fn map_crypto_error(error: TransportCryptoError) -> Error {
    match error {
        TransportCryptoError::UnsupportedAlgorithm => {
            Error::UnsupportedAlgorithm("closed-set algorithm required".into())
        }
        TransportCryptoError::Crypto(CryptoError::LegacyNotAllowed { id }) => {
            Error::LegacyDenied(id.as_str().to_owned())
        }
        TransportCryptoError::Crypto(CryptoError::UnknownAlgorithmId { raw }) => {
            Error::UnsupportedAlgorithm(raw)
        }
        TransportCryptoError::Crypto(CryptoError::NotRegistered { id }) => {
            Error::UnsupportedAlgorithm(id.as_str().to_owned())
        }
        TransportCryptoError::InvalidProfile(reason) => {
            Error::Gateway(format!("invalid transport profile or envelope: {reason}"))
        }
        other => Error::Crypto(other),
    }
}

/// 按传输模式解码网关响应体。
///
/// - `full`：成功响应必须是 [`TransportEnvelope`]（由 `open` 解开）；网关在解密 / 鉴权 /
///   请求绑定阶段的拒绝（非 2xx）以明文错误体返回，按明文解析以保留错误码；
///   2xx 却是明文 → 视为降级，报错。
/// - `none` / `request`：响应必须是明文；收到信封视为配置不一致，报错。
fn decode_gateway_payload(
    mode: TransportMode,
    http_status: u16,
    raw: &[u8],
    open: impl FnOnce(&TransportEnvelope) -> Result<Vec<u8>, Error>,
) -> Result<GatewayResponseEnvelope, Error> {
    let deserialize = |bytes: &[u8]| {
        serde_json::from_slice::<GatewayResponseEnvelope>(bytes).map_err(Error::from)
    };
    match (mode, serde_json::from_slice::<TransportEnvelope>(raw)) {
        (TransportMode::Full, Ok(envelope)) => deserialize(&open(&envelope)?),
        (TransportMode::Full, Err(_)) if !(200..300).contains(&http_status) => deserialize(raw),
        (TransportMode::Full, Err(error)) => Err(Error::Gateway(format!(
            "mode=full expects TransportEnvelope response: {error}"
        ))),
        (mode, Ok(envelope)) => Err(Error::Gateway(format!(
            "unexpected encrypted response under mode={mode:?} (data_alg={})",
            envelope.data_alg
        ))),
        (_, Err(_)) => deserialize(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x_loadgen_core::crypto::request_binding::{
        HEADER_KEY_ID, HEADER_NONCE, HEADER_SIGNATURE, HEADER_SIGNATURE_INPUT, HEADER_TIMESTAMP,
        binding_key_ref, canonical_http_binding, parse_signature_input, verify_binding,
    };
    use x_loadgen_core::crypto::{
        AlgorithmId, KeySelection, SUITE_TENANT_DEFAULT_2026_11, generate_sign_keypair,
    };

    fn aes_full_profile() -> TenantTransportProfile {
        let mut profile = TenantTransportProfile::from_named_suite(
            SUITE_TENANT_DEFAULT_2026_11,
            TransportMode::Full,
        )
        .expect("suite");
        profile.key = Some(KeySelection::Latest {
            key_id: "tenant-default".into(),
        });
        profile
    }

    fn session(transport: TenantTransportProfile, signer: Option<RequestSigner>) -> GatewaySession {
        GatewaySession::new(SessionConfig {
            tenant_id: "00000000-0000-0000-0000-000000000001".into(),
            tenant_key: vec![7u8; 32],
            key_version: "v1".into(),
            transport,
            request_signer: signer,
        })
        .expect("session")
    }

    #[test]
    fn full_mode_keeps_plaintext_rejections_but_refuses_plaintext_success() {
        let rejected = br#"{"success":false,"error":"40113 signature invalid","status":401}"#;
        let decoded = decode_gateway_payload(TransportMode::Full, 401, rejected, |_| {
            panic!("plaintext rejection must not be opened")
        })
        .expect("plaintext rejection");
        assert!(!decoded.success);
        assert_eq!(decoded.status, 401);
        assert!(decoded.error.unwrap().contains("40113"));

        let ok_plain = br#"{"success":true,"data":{},"status":200}"#;
        assert!(
            decode_gateway_payload(TransportMode::Full, 200, ok_plain, |_| unreachable!()).is_err()
        );
        assert!(
            decode_gateway_payload(TransportMode::None, 200, ok_plain, |_| unreachable!())
                .expect("plain")
                .success
        );
    }

    #[test]
    fn non_full_modes_reject_encrypted_responses() {
        let registry = AlgorithmRegistry::default_registry();
        let profile = aes_full_profile();
        let sealed = seal_envelope(SealEnvelopeParams {
            profile: &profile,
            registry: &registry,
            data_key: &[7u8; 32],
            mac_key: None,
            key_version: "v1",
            tenant_id: "tenant-1",
            http_path: GATEWAY_INVOKE_PATH,
            plaintext: br#"{"success":true,"status":200}"#,
            timestamp: 1_710_000_000,
        })
        .expect("seal");
        let raw = serde_json::to_vec(&sealed).expect("json");
        for mode in [TransportMode::None, TransportMode::Request] {
            assert!(decode_gateway_payload(mode, 200, &raw, |_| unreachable!()).is_err());
        }
        let opened = decode_gateway_payload(TransportMode::Full, 200, &raw, |env| {
            open_envelope(
                &profile,
                &registry,
                &[7u8; 32],
                None,
                "tenant-1",
                GATEWAY_INVOKE_PATH,
                env,
            )
            .map_err(Error::from)
        })
        .expect("open");
        assert!(opened.success);
    }

    #[test]
    fn mode_none_profile_validates_without_algs() {
        let registry = AlgorithmRegistry::default_registry();
        let profile = TenantTransportProfile::plaintext();
        profile.validate(&registry).expect("plaintext ok");
        assert!(matches!(profile.mode, TransportMode::None));
    }

    #[test]
    fn full_session_roundtrips_through_peer_seal() {
        let sdk = session(aes_full_profile(), None);
        let encoded = sdk
            .encode_request(br#"{"product_id":"demo"}"#, 1_710_000_000)
            .expect("encode");
        let envelope: TransportEnvelope = serde_json::from_slice(&encoded.body).expect("envelope");
        assert_eq!(envelope.data_alg, "aes-256-gcm");
        assert!(envelope.mac_alg.is_none());
        let registry = AlgorithmRegistry::default_registry();
        let opened = open_envelope(
            &aes_full_profile(),
            &registry,
            &[7u8; 32],
            None,
            "00000000-0000-0000-0000-000000000001",
            GATEWAY_INVOKE_PATH,
            &envelope,
        )
        .expect("peer open");
        assert_eq!(opened, br#"{"product_id":"demo"}"#);

        let reply = seal_envelope(SealEnvelopeParams {
            profile: &aes_full_profile(),
            registry: &registry,
            data_key: &[7u8; 32],
            mac_key: None,
            key_version: "v1",
            tenant_id: "00000000-0000-0000-0000-000000000001",
            http_path: GATEWAY_INVOKE_PATH,
            plaintext: br#"{"success":true,"data":{"ok":1},"status":200}"#,
            timestamp: 1_710_000_000,
        })
        .expect("reply");
        let decoded = sdk
            .decode_response(200, &serde_json::to_vec(&reply).expect("json"))
            .expect("decode");
        assert_eq!(decoded.data, Some(serde_json::json!({"ok": 1})));
    }

    #[test]
    fn legacy_alg_without_allow_insecure_is_denied() {
        let mut profile = aes_full_profile();
        profile.mode = TransportMode::Request;
        profile.suite = None;
        profile.data_alg = Some(AlgorithmId::Aes256Ecb);
        profile.mac_alg = Some(AlgorithmId::HmacSha256);
        profile.allow_insecure = false;
        let err = GatewaySession::new(SessionConfig {
            tenant_id: "t".into(),
            tenant_key: vec![1u8; 32],
            key_version: "v1".into(),
            transport: profile,
            request_signer: None,
        })
        .expect_err("legacy denied");
        assert!(matches!(err, Error::LegacyDenied(id) if id == "aes-256-ecb"));
    }

    #[test]
    fn map_crypto_error_surfaces_legacy_and_unsupported() {
        match map_crypto_error(TransportCryptoError::Crypto(
            CryptoError::LegacyNotAllowed {
                id: AlgorithmId::Aes256Ecb,
            },
        )) {
            Error::LegacyDenied(id) => assert_eq!(id, "aes-256-ecb"),
            other => panic!("unexpected {other:?}"),
        }
        match map_crypto_error(TransportCryptoError::UnsupportedAlgorithm) {
            Error::UnsupportedAlgorithm(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn request_binding_headers_verify_with_registered_public_key() {
        for alg in [
            AlgorithmId::Ed25519,
            AlgorithmId::Sm2Sign,
            AlgorithmId::EcdsaP256Sha256,
        ] {
            let pair = generate_sign_keypair(alg).expect("keygen");
            let signer = RequestSigner::new("3", alg, pair.private_pem.as_bytes()).expect("signer");
            let sdk = session(TenantTransportProfile::plaintext(), Some(signer));
            let body = br#"{"product_id":"p1","path":"/x"}"#;
            let encoded = sdk.encode_request(body, 0).expect("encode");
            assert_eq!(encoded.body, body);
            let header = |name: &str| {
                encoded
                    .headers
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map_or_else(|| panic!("missing {name}"), |(_, value)| value.clone())
            };
            assert_eq!(header(HEADER_KEY_ID), "3");
            assert_eq!(
                parse_signature_input(&header(HEADER_SIGNATURE_INPUT)).expect("input"),
                alg
            );
            let timestamp: i64 = header(HEADER_TIMESTAMP).parse().expect("ts");
            let canonical = canonical_http_binding(
                "POST",
                GATEWAY_INVOKE_PATH,
                timestamp,
                &header(HEADER_NONCE),
                body,
            );
            let public = binding_key_ref(alg, pair.public_pem.as_bytes()).expect("public");
            verify_binding(
                &AlgorithmRegistry::default_registry(),
                alg,
                &public,
                &canonical,
                &header(HEADER_SIGNATURE),
            )
            .unwrap_or_else(|e| panic!("{alg}: {e}"));
        }
    }

    #[test]
    fn request_binding_covers_sealed_body() {
        let signer = RequestSigner::new("9", AlgorithmId::HmacSha256, &[5u8; 32]).expect("signer");
        let sdk = session(aes_full_profile(), Some(signer));
        let encoded = sdk.encode_request(b"{}", 1_710_000_000).expect("encode");
        assert!(serde_json::from_slice::<TransportEnvelope>(&encoded.body).is_ok());
        let header = |name: &str| {
            encoded
                .headers
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        let canonical = canonical_http_binding(
            "POST",
            GATEWAY_INVOKE_PATH,
            header(HEADER_TIMESTAMP).parse().expect("ts"),
            &header(HEADER_NONCE),
            &encoded.body,
        );
        let key = binding_key_ref(AlgorithmId::HmacSha256, &[5u8; 32]).expect("key");
        verify_binding(
            &AlgorithmRegistry::default_registry(),
            AlgorithmId::HmacSha256,
            &key,
            &canonical,
            &header(HEADER_SIGNATURE),
        )
        .expect("binding covers sealed body");
    }

    #[test]
    fn request_binding_absent_without_signer() {
        let sdk = session(TenantTransportProfile::plaintext(), None);
        let encoded = sdk.encode_request(b"{}", 0).expect("encode");
        assert_eq!(encoded.headers, Vec::new());
        assert!(!sdk.has_binding());
    }
}
