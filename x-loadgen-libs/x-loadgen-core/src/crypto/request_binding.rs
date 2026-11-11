//! HTTP 请求绑定：对 `METHOD\npath_and_query\nts\nnonce\nbody` 做 MAC 或非对称签名。
//!
//! - 协议档（租户 profile 的 `signature_version`）：[`SignatureVersion`]，限定可用的密钥算法族；
//! - 每把请求密钥登记自身算法 id（闭集，见 [`binding_alg_allowed`]）；
//! - 请求头 `X-Mazbri-Signature-Input` 固定为 `{alg}=v1`（[`signature_input_for`]），
//!   服务端按 kid 查到的密钥算法做精确比对，防止算法混淆；
//! - 签名统一标准 Base64。ECDSA 为 IEEE P1363 `r‖s`，SM2 为 DER（默认 `user_id`），
//!   RSA 为 PKCS#1 v1.5，Ed25519 为 64 字节原始签名。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use x_loadgen_crypto::id::{AlgorithmId, AlgorithmKind};
use x_loadgen_crypto::material::{KeyPurpose, KeyRef};
use x_loadgen_crypto::parse_algorithm_id;
use x_loadgen_crypto::registry::AlgorithmRegistry;
use x_loadgen_crypto::traits::{CryptoError, Mac};

/// 请求头：Unix 秒时间戳。
pub const HEADER_TIMESTAMP: &str = "x-mazbri-timestamp";
/// 请求头：一次性 nonce。
pub const HEADER_NONCE: &str = "x-mazbri-nonce";
/// 请求头：签名（标准 Base64）。
pub const HEADER_SIGNATURE: &str = "x-mazbri-signature";
/// 请求头：签名输入协议（`{alg}=v1`）。
pub const HEADER_SIGNATURE_INPUT: &str = "x-mazbri-signature-input";
/// 请求头：请求密钥 kid。
pub const HEADER_KEY_ID: &str = "x-mazbri-key-id";
/// 签名输入协议版本后缀。
pub const SIGNATURE_INPUT_SUFFIX: &str = "=v1";
/// 默认绑定 MAC 算法 id（kebab-case）。
pub const DEFAULT_BINDING_MAC_ALG: &str = "hmac-sha256";
/// Nonce 最小长度。
pub const NONCE_MIN_LEN: usize = 16;
/// Nonce 最大长度。
pub const NONCE_MAX_LEN: usize = 128;
/// 默认时钟偏移（秒）。
pub const DEFAULT_CLOCK_SKEW_SECONDS: i64 = 300;
/// 默认 nonce TTL（秒）。
pub const DEFAULT_NONCE_TTL_SECONDS: i32 = 300;

/// 租户请求绑定协议档（profile `signature_version`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureVersion {
    /// 对称 HMAC（hmac-sha256/384/512、hmac-sm3）。
    HmacV1,
    /// RSA PKCS#1 v1.5（sha256/384/512）。
    RsaV1,
    /// ECDSA P-256 / P-384。
    EcdsaV1,
    /// Ed25519。
    Ed25519V1,
    /// 国密 SM2 签名。
    Sm2V1,
}

impl SignatureVersion {
    /// 全部协议档。
    pub const ALL: [Self; 5] = [
        Self::HmacV1,
        Self::RsaV1,
        Self::EcdsaV1,
        Self::Ed25519V1,
        Self::Sm2V1,
    ];

    /// 解析 `signature_version`（kebab-case，精确匹配）。
    ///
    /// # Errors
    ///
    /// 非已知协议档时返回 [`RequestBindingError::UnsupportedSignatureVersion`]。
    pub fn parse(raw: &str) -> Result<Self, RequestBindingError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == raw)
            .ok_or(RequestBindingError::UnsupportedSignatureVersion)
    }

    /// 线上字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HmacV1 => "hmac-v1",
            Self::RsaV1 => "rsa-v1",
            Self::EcdsaV1 => "ecdsa-v1",
            Self::Ed25519V1 => "ed25519-v1",
            Self::Sm2V1 => "sm2-v1",
        }
    }

    /// 是否对称协议档（密钥由平台生成、仅展示一次）。
    #[must_use]
    pub const fn is_symmetric(self) -> bool {
        matches!(self, Self::HmacV1)
    }

    /// 协议档的默认密钥算法。
    #[must_use]
    pub const fn default_alg(self) -> AlgorithmId {
        match self {
            Self::HmacV1 => AlgorithmId::HmacSha256,
            Self::RsaV1 => AlgorithmId::RsaPkcs1v15SignSha256,
            Self::EcdsaV1 => AlgorithmId::EcdsaP256Sha256,
            Self::Ed25519V1 => AlgorithmId::Ed25519,
            Self::Sm2V1 => AlgorithmId::Sm2Sign,
        }
    }

    /// 该协议档是否允许此密钥算法。
    #[must_use]
    pub const fn allows(self, alg: AlgorithmId) -> bool {
        matches!(
            (self, alg),
            (
                Self::HmacV1,
                AlgorithmId::HmacSha256
                    | AlgorithmId::HmacSha384
                    | AlgorithmId::HmacSha512
                    | AlgorithmId::HmacSm3
            ) | (
                Self::RsaV1,
                AlgorithmId::RsaPkcs1v15SignSha256
                    | AlgorithmId::RsaPkcs1v15SignSha384
                    | AlgorithmId::RsaPkcs1v15SignSha512
            ) | (
                Self::EcdsaV1,
                AlgorithmId::EcdsaP256Sha256 | AlgorithmId::EcdsaP384Sha384
            ) | (Self::Ed25519V1, AlgorithmId::Ed25519)
                | (Self::Sm2V1, AlgorithmId::Sm2Sign)
        )
    }

    /// 由密钥算法反推协议档（闭集外返回 `None`）。
    #[must_use]
    pub fn for_alg(alg: AlgorithmId) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.allows(alg))
    }
}

impl std::fmt::Display for SignatureVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 请求绑定密钥算法是否在闭集内（`hmac-md5` 等不安全算法不允许）。
#[must_use]
pub fn binding_alg_allowed(alg: AlgorithmId) -> bool {
    SignatureVersion::for_alg(alg).is_some()
}

/// 解析请求绑定密钥算法（kebab-case 闭集）。
///
/// # Errors
///
/// 非闭集 id 时返回 [`RequestBindingError::UnsupportedAlgorithm`]。
pub fn parse_binding_alg(raw: &str) -> Result<AlgorithmId, RequestBindingError> {
    let id = parse_algorithm_id(raw).map_err(|_| RequestBindingError::UnsupportedAlgorithm)?;
    if binding_alg_allowed(id) {
        Ok(id)
    } else {
        Err(RequestBindingError::UnsupportedAlgorithm)
    }
}

/// `X-Mazbri-Signature-Input` 取值：`{alg}=v1`。
#[must_use]
pub fn signature_input_for(alg: AlgorithmId) -> String {
    format!("{}{SIGNATURE_INPUT_SUFFIX}", alg.as_str())
}

/// 解析 `X-Mazbri-Signature-Input`，返回声明的算法。
///
/// # Errors
///
/// 格式非法或算法不在闭集时返回 [`RequestBindingError::UnsupportedSignatureInput`]。
pub fn parse_signature_input(value: &str) -> Result<AlgorithmId, RequestBindingError> {
    let alg = value
        .strip_suffix(SIGNATURE_INPUT_SUFFIX)
        .ok_or(RequestBindingError::UnsupportedSignatureInput)?;
    parse_binding_alg(alg).map_err(|_| RequestBindingError::UnsupportedSignatureInput)
}

/// 请求绑定错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RequestBindingError {
    /// 密钥长度 / 材料非法。
    #[error("invalid binding key")]
    InvalidKey,
    /// Nonce 非法。
    #[error("invalid nonce")]
    InvalidNonce,
    /// 时间戳非法。
    #[error("invalid timestamp")]
    InvalidTimestamp,
    /// 超出允许时钟偏移。
    #[error("timestamp outside allowed skew")]
    TimestampSkew,
    /// 签名编码非法。
    #[error("invalid signature encoding")]
    InvalidSignatureEncoding,
    /// 签名不匹配。
    #[error("signature mismatch")]
    SignatureMismatch,
    /// 不支持的 signature-input 协议档。
    #[error("unsupported signature input")]
    UnsupportedSignatureInput,
    /// 不支持的绑定算法 id。
    #[error("unsupported binding algorithm")]
    UnsupportedAlgorithm,
    /// 不支持的协议档（`signature_version`）。
    #[error("unsupported signature version")]
    UnsupportedSignatureVersion,
}

#[must_use]
pub fn canonical_http_binding(
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    body: &[u8],
) -> String {
    let mut payload = String::with_capacity(
        method.len() + path_and_query.len() + 20 + nonce.len() + body.len() + 4,
    );
    payload.push_str(method);
    payload.push('\n');
    payload.push_str(path_and_query);
    payload.push('\n');
    payload.push_str(&timestamp.to_string());
    payload.push('\n');
    payload.push_str(nonce);
    payload.push('\n');
    payload.push_str(&String::from_utf8_lossy(body));
    payload
}

#[must_use]
pub fn path_and_query_from_parts(path: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !q.is_empty() => format!("{path}?{q}"),
        _ => path.to_owned(),
    }
}

/// # Errors
///
/// Nonce 长度或字符集非法时返回错误。
pub fn validate_binding_nonce(nonce: &str) -> Result<(), RequestBindingError> {
    if nonce.len() < NONCE_MIN_LEN || nonce.len() > NONCE_MAX_LEN {
        return Err(RequestBindingError::InvalidNonce);
    }
    if !nonce
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    {
        return Err(RequestBindingError::InvalidNonce);
    }
    Ok(())
}

/// # Errors
///
/// 非整数时返回错误。
pub fn parse_unix_timestamp(value: &str) -> Result<i64, RequestBindingError> {
    value
        .parse::<i64>()
        .map_err(|_| RequestBindingError::InvalidTimestamp)
}

/// # Errors
///
/// 时间戳非正或超出偏移时返回错误。
pub fn validate_unix_timestamp(
    timestamp: i64,
    now_unix: i64,
    clock_skew_seconds: i64,
) -> Result<(), RequestBindingError> {
    if timestamp <= 0 {
        return Err(RequestBindingError::InvalidTimestamp);
    }
    let skew = clock_skew_seconds.max(1);
    if (now_unix - timestamp).abs() > skew {
        return Err(RequestBindingError::TimestampSkew);
    }
    Ok(())
}

fn map_key_error(err: &CryptoError) -> RequestBindingError {
    match err {
        CryptoError::InvalidKey { .. } => RequestBindingError::InvalidKey,
        _ => RequestBindingError::UnsupportedAlgorithm,
    }
}

fn key_purpose(alg: AlgorithmId) -> KeyPurpose {
    if alg.kind() == AlgorithmKind::Mac {
        KeyPurpose::Mac
    } else {
        KeyPurpose::AsymSign
    }
}

/// 构造绑定密钥引用：MAC 为原始字节；签名算法按 PEM / Base64 文本约定
///（见 [`x_loadgen_crypto::key_ref_from_text`]）。
///
/// # Errors
///
/// 算法不在闭集、或签名密钥文本非 UTF-8 / 非法时返回错误。
pub fn binding_key_ref(alg: AlgorithmId, material: &[u8]) -> Result<KeyRef, RequestBindingError> {
    if !binding_alg_allowed(alg) {
        return Err(RequestBindingError::UnsupportedAlgorithm);
    }
    if alg.kind() == AlgorithmKind::Mac {
        return Ok(KeyRef::raw(KeyPurpose::Mac, material.to_vec()));
    }
    let text = std::str::from_utf8(material).map_err(|_| RequestBindingError::InvalidKey)?;
    x_loadgen_crypto::key_ref_from_text(key_purpose(alg), text).map_err(|e| map_key_error(&e))
}

/// 对绑定载荷签名（MAC 或非对称），返回标准 Base64。
///
/// # Errors
///
/// 算法不在闭集 / 未登记，或密钥非法时返回错误。
pub fn sign_binding(
    registry: &AlgorithmRegistry,
    alg: AlgorithmId,
    key: &KeyRef,
    payload: &str,
) -> Result<String, RequestBindingError> {
    if !binding_alg_allowed(alg) {
        return Err(RequestBindingError::UnsupportedAlgorithm);
    }
    let tag = if alg.kind() == AlgorithmKind::Mac {
        let mac = registry
            .mac(alg)
            .map_err(|_| RequestBindingError::UnsupportedAlgorithm)?;
        mac.sign(key, payload.as_bytes())
    } else {
        let signer = x_loadgen_crypto::asym_signer(alg)
            .map_err(|_| RequestBindingError::UnsupportedAlgorithm)?;
        signer.sign(key, payload.as_bytes())
    }
    .map_err(|e| map_key_error(&e))?;
    Ok(STANDARD.encode(tag))
}

/// 校验绑定签名（标准 Base64）。
///
/// # Errors
///
/// 编码非法、密钥非法或签名不匹配时返回错误。
pub fn verify_binding(
    registry: &AlgorithmRegistry,
    alg: AlgorithmId,
    key: &KeyRef,
    payload: &str,
    signature_b64: &str,
) -> Result<(), RequestBindingError> {
    if !binding_alg_allowed(alg) {
        return Err(RequestBindingError::UnsupportedAlgorithm);
    }
    let actual = STANDARD
        .decode(signature_b64.trim().as_bytes())
        .map_err(|_| RequestBindingError::InvalidSignatureEncoding)?;
    let result = if alg.kind() == AlgorithmKind::Mac {
        let mac = registry
            .mac(alg)
            .map_err(|_| RequestBindingError::UnsupportedAlgorithm)?;
        mac.verify(key, payload.as_bytes(), &actual)
    } else {
        let signer = x_loadgen_crypto::asym_signer(alg)
            .map_err(|_| RequestBindingError::UnsupportedAlgorithm)?;
        signer.verify(key, payload.as_bytes(), &actual)
    };
    result.map_err(|err| match err {
        CryptoError::AuthenticationFailed | CryptoError::InvalidInput { .. } => {
            RequestBindingError::SignatureMismatch
        }
        other => map_key_error(&other),
    })
}

/// 非对称请求密钥公钥完整性摘要的作用域（与对称密钥摘要做域分离）。
///
/// mgmt 以 `secret_key_digest(integrity, tenant, scope, public_key_text)` 入库，
/// gateway 以同一作用域校验，防止公钥被直接改库替换。
#[must_use]
pub fn public_key_digest_scope(kid: &str) -> String {
    format!("request-public-key:{kid}")
}

/// 一次请求的绑定头（客户端生成、服务端测试复用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingHeaders {
    /// Unix 秒。
    pub timestamp: i64,
    /// nonce。
    pub nonce: String,
    /// 签名（标准 Base64）。
    pub signature: String,
    /// `{alg}=v1`。
    pub signature_input: String,
    /// kid。
    pub key_id: String,
}

impl BindingHeaders {
    /// 以 (头名, 值) 形式列出全部绑定头（头名小写）。
    #[must_use]
    pub fn pairs(&self) -> [(&'static str, String); 5] {
        [
            (HEADER_TIMESTAMP, self.timestamp.to_string()),
            (HEADER_NONCE, self.nonce.clone()),
            (HEADER_SIGNATURE, self.signature.clone()),
            (HEADER_SIGNATURE_INPUT, self.signature_input.clone()),
            (HEADER_KEY_ID, self.key_id.clone()),
        ]
    }
}

/// 客户端请求绑定签名器：kid + 算法 + 密钥（MAC 原始字节或签名私钥）。
#[derive(Clone)]
pub struct RequestSigner {
    kid: String,
    alg: AlgorithmId,
    key: KeyRef,
    registry: AlgorithmRegistry,
}

impl std::fmt::Debug for RequestSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestSigner")
            .field("kid", &self.kid)
            .field("alg", &self.alg)
            .field("key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl RequestSigner {
    /// 由 kid、算法与密钥材料构造（MAC：原始字节；签名：私钥 PEM / Base64 文本字节）。
    ///
    /// # Errors
    ///
    /// kid 为空、算法不在闭集或密钥非法时返回错误。
    pub fn new(
        kid: impl Into<String>,
        alg: AlgorithmId,
        material: &[u8],
    ) -> Result<Self, RequestBindingError> {
        let kid = kid.into();
        if kid.trim().is_empty() {
            return Err(RequestBindingError::InvalidKey);
        }
        let key = binding_key_ref(alg, material)?;
        let signer = Self {
            kid,
            alg,
            key,
            registry: AlgorithmRegistry::default_registry(),
        };
        signer.sign_payload("mazbri-request-signer-self-check")?;
        Ok(signer)
    }

    /// kid。
    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// 算法。
    #[must_use]
    pub const fn alg(&self) -> AlgorithmId {
        self.alg
    }

    fn sign_payload(&self, payload: &str) -> Result<String, RequestBindingError> {
        sign_binding(&self.registry, self.alg, &self.key, payload)
    }

    /// 以指定时间戳与 nonce 签名（便于测试复现）。
    ///
    /// # Errors
    ///
    /// 签名失败时返回错误。
    pub fn sign_with(
        &self,
        method: &str,
        path_and_query: &str,
        timestamp: i64,
        nonce: String,
        body: &[u8],
    ) -> Result<BindingHeaders, RequestBindingError> {
        let canonical = canonical_http_binding(method, path_and_query, timestamp, &nonce, body);
        Ok(BindingHeaders {
            timestamp,
            signature: self.sign_payload(&canonical)?,
            nonce,
            signature_input: signature_input_for(self.alg),
            key_id: self.kid.clone(),
        })
    }

    /// 以当前时间与随机 nonce 签名。
    ///
    /// # Errors
    ///
    /// 签名失败时返回错误。
    pub fn sign(
        &self,
        method: &str,
        path_and_query: &str,
        body: &[u8],
    ) -> Result<BindingHeaders, RequestBindingError> {
        self.sign_with(
            method,
            path_and_query,
            unix_now_secs(),
            generate_binding_nonce(),
            body,
        )
    }
}

/// 当前 Unix 秒（系统时钟早于 1970 时为 0，超出 `i64` 时饱和）。
#[must_use]
pub fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[must_use]
pub fn generate_binding_nonce() -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let nonce: [u8; 16] = rand::random();
    URL_SAFE_NO_PAD.encode(nonce)
}

#[must_use]
pub fn truncate_nonce_for_log(nonce: &str) -> String {
    const KEEP: usize = 8;
    if nonce.len() <= KEEP {
        nonce.to_owned()
    } else {
        format!("{}…", &nonce[..KEEP])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: &str = "abcdefABCDEF12-_";

    fn canonical(body: &[u8]) -> String {
        canonical_http_binding("POST", "/api/v1/gateway", 1_710_000_000, NONCE, body)
    }

    #[test]
    fn canonical_matches_scheme_vector() {
        let payload = canonical(br#"{"product_id":"p1","path":"/x"}"#);
        assert_eq!(
            payload,
            format!(
                "POST\n/api/v1/gateway\n1710000000\n{NONCE}\n{{\"product_id\":\"p1\",\"path\":\"/x\"}}"
            )
        );
    }

    #[test]
    fn hmac_sha256_known_vector_is_stable() {
        let registry = AlgorithmRegistry::default_registry();
        let key = KeyRef::raw(KeyPurpose::Mac, vec![7u8; 32]);
        let sig =
            sign_binding(&registry, AlgorithmId::HmacSha256, &key, &canonical(b"{}")).unwrap();
        assert_eq!(STANDARD.decode(&sig).unwrap().len(), 32);
        assert_eq!(
            signature_input_for(AlgorithmId::HmacSha256),
            "hmac-sha256=v1"
        );
    }

    #[test]
    fn every_binding_alg_roundtrips_and_rejects_tamper() {
        let registry = AlgorithmRegistry::default_registry();
        for version in SignatureVersion::ALL {
            let alg = version.default_alg();
            assert!(version.allows(alg));
            let (signing, verifying) = if version.is_symmetric() {
                (vec![9u8; 32], vec![9u8; 32])
            } else {
                let pair = x_loadgen_crypto::generate_sign_keypair(alg).unwrap();
                (
                    pair.private_pem.as_bytes().to_vec(),
                    pair.public_pem.into_bytes(),
                )
            };
            let signer = RequestSigner::new("7", alg, &signing).unwrap();
            let headers = signer
                .sign_with(
                    "POST",
                    "/api/v1/gateway",
                    1_710_000_000,
                    NONCE.to_owned(),
                    b"{}",
                )
                .unwrap();
            assert_eq!(headers.key_id, "7");
            assert_eq!(
                parse_signature_input(&headers.signature_input).unwrap(),
                alg
            );
            let key = binding_key_ref(alg, &verifying).unwrap();
            verify_binding(&registry, alg, &key, &canonical(b"{}"), &headers.signature)
                .unwrap_or_else(|e| panic!("{version}: {e}"));
            assert_eq!(
                verify_binding(
                    &registry,
                    alg,
                    &key,
                    &canonical(b"{\"x\":1}"),
                    &headers.signature
                ),
                Err(RequestBindingError::SignatureMismatch),
                "{version}"
            );
        }
    }

    #[test]
    fn binding_header_names_are_stable() {
        assert_eq!(
            [
                HEADER_TIMESTAMP,
                HEADER_NONCE,
                HEADER_SIGNATURE,
                HEADER_SIGNATURE_INPUT,
                HEADER_KEY_ID
            ],
            [
                "x-mazbri-timestamp",
                "x-mazbri-nonce",
                "x-mazbri-signature",
                "x-mazbri-signature-input",
                "x-mazbri-key-id"
            ]
        );
        assert_eq!(signature_input_for(AlgorithmId::Sm2Sign), "sm2-sign=v1");
        assert_eq!(
            signature_input_for(AlgorithmId::RsaPkcs1v15SignSha384),
            "rsa-pkcs1v15-sign-sha384=v1"
        );
    }

    #[test]
    fn hmac_sm3_roundtrip() {
        let registry = AlgorithmRegistry::default_registry();
        let key = binding_key_ref(AlgorithmId::HmacSm3, &[9u8; 32]).unwrap();
        let payload = canonical(b"{}");
        let sig = sign_binding(&registry, AlgorithmId::HmacSm3, &key, &payload).unwrap();
        verify_binding(&registry, AlgorithmId::HmacSm3, &key, &payload, &sig).unwrap();
        let wrong = binding_key_ref(AlgorithmId::HmacSm3, &[8u8; 32]).unwrap();
        assert_eq!(
            verify_binding(&registry, AlgorithmId::HmacSm3, &wrong, &payload, &sig),
            Err(RequestBindingError::SignatureMismatch)
        );
    }

    #[test]
    fn version_family_closed_set() {
        assert_eq!(
            SignatureVersion::parse("rsa-v1").unwrap(),
            SignatureVersion::RsaV1
        );
        assert_eq!(
            SignatureVersion::parse("RSA-V1"),
            Err(RequestBindingError::UnsupportedSignatureVersion)
        );
        assert!(!SignatureVersion::HmacV1.allows(AlgorithmId::Ed25519));
        assert!(!SignatureVersion::RsaV1.allows(AlgorithmId::HmacSha256));
        assert!(!SignatureVersion::EcdsaV1.allows(AlgorithmId::Sm2Sign));
        assert!(!binding_alg_allowed(AlgorithmId::HmacMd5));
        assert_eq!(
            parse_binding_alg("hmac-md5"),
            Err(RequestBindingError::UnsupportedAlgorithm)
        );
        assert_eq!(
            parse_binding_alg("aes-256-gcm"),
            Err(RequestBindingError::UnsupportedAlgorithm)
        );
        for version in SignatureVersion::ALL {
            assert_eq!(SignatureVersion::parse(version.as_str()).unwrap(), version);
            assert_eq!(
                SignatureVersion::for_alg(version.default_alg()),
                Some(version)
            );
        }
    }

    #[test]
    fn signature_input_parsing() {
        assert_eq!(
            parse_signature_input("hmac-sha256=v1").unwrap(),
            AlgorithmId::HmacSha256
        );
        assert_eq!(
            parse_signature_input("sm2-sign=v1").unwrap(),
            AlgorithmId::Sm2Sign
        );
        for bad in [
            "hmac-sha256",
            "hmac-sha256=v2",
            "hmac-md5=v1",
            "=v1",
            "HMAC-SHA256=v1",
        ] {
            assert_eq!(
                parse_signature_input(bad),
                Err(RequestBindingError::UnsupportedSignatureInput),
                "{bad}"
            );
        }
    }

    #[test]
    fn signer_rejects_bad_material_and_redacts_debug() {
        assert!(RequestSigner::new("", AlgorithmId::HmacSha256, &[1u8; 32]).is_err());
        assert!(RequestSigner::new("1", AlgorithmId::HmacMd5, &[1u8; 32]).is_err());
        assert!(RequestSigner::new("1", AlgorithmId::Ed25519, b"not-a-key").is_err());
        let signer = RequestSigner::new("1", AlgorithmId::HmacSha256, &[1u8; 32]).unwrap();
        assert!(!format!("{signer:?}").contains("1, 1"));
        let headers = signer.sign("POST", "/api/v1/gateway", b"{}").unwrap();
        validate_binding_nonce(&headers.nonce).unwrap();
        assert_eq!(headers.pairs().len(), 5);
    }

    #[test]
    fn nonce_charset_and_length() {
        assert!(validate_binding_nonce("abcdefghijklmnop").is_ok());
        assert!(validate_binding_nonce("ab").is_err());
        assert!(validate_binding_nonce(&"a".repeat(129)).is_err());
        assert!(validate_binding_nonce("bad nonce spaces!!").is_err());
    }

    #[test]
    fn timestamp_skew_window() {
        validate_unix_timestamp(1000, 1000, 300).unwrap();
        validate_unix_timestamp(700, 1000, 300).unwrap();
        assert_eq!(
            validate_unix_timestamp(699, 1000, 300),
            Err(RequestBindingError::TimestampSkew)
        );
    }
}
