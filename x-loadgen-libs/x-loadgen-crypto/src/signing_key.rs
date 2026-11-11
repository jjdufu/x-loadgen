//! 签名密钥对：文本编码、生成、公钥校验与统一签名 / 验签分派。
//!
//! 文本编码约定（请求绑定公钥登记、客户端私钥配置共用）：
//! - 以 `-----BEGIN` 开头：PEM。公钥为 SPKI（`PUBLIC KEY`，RSA 另接受 PKCS#1），
//!   私钥为 PKCS#8（`PRIVATE KEY`，RSA 另接受 PKCS#1）。SM2 使用
//!   id-ecPublicKey + sm2p256v1（与 `OpenSSL` 3 一致）。
//! - 否则按标准 Base64 解码为原始字节（ECDSA 标量 / SEC1 点、Ed25519 种子 / 公钥、
//!   SM2 标量 / SEC1 点）；RSA 不支持原始字节。
//!
//! [`generate_sign_keypair`] 统一输出 PEM。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::rand_core::{OsRng, RngCore};
use rsa::traits::PublicKeyParts;
use zeroize::Zeroizing;

use crate::asym::{
    AsymSignAlgorithm, parse_ed25519_verifying, parse_p256_verifying, parse_p384_verifying,
    parse_rsa_public,
};
use crate::id::{AlgorithmId, AlgorithmKind};
use crate::material::{KeyPurpose, KeyRef};
use crate::sm::{
    Sm2SignAlgorithm, parse_sm2_public, sm2_private_key_to_pem, sm2_public_key_to_pem,
};
use crate::traits::{AsymSign, CryptoError};

/// 生成 RSA 签名密钥的模长（位）。
pub const RSA_SIGN_KEY_BITS: usize = 2048;
/// 接受登记的 RSA 公钥最小模长（位）。
pub const RSA_SIGN_MIN_BITS: usize = 2048;

/// 生成的签名密钥对（PEM 文本）；私钥在 drop 时清零。
pub struct SignKeyPair {
    /// 算法 id。
    pub alg: AlgorithmId,
    /// PKCS#8 私钥 PEM。
    pub private_pem: Zeroizing<String>,
    /// SPKI 公钥 PEM。
    pub public_pem: String,
}

impl std::fmt::Debug for SignKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignKeyPair")
            .field("alg", &self.alg)
            .field("private_pem", &"<redacted>")
            .field("public_pem", &self.public_pem)
            .finish()
    }
}

fn ensure_sign_alg(id: AlgorithmId) -> Result<(), CryptoError> {
    if id.kind() == AlgorithmKind::AsymSign {
        Ok(())
    } else {
        Err(CryptoError::Unsupported {
            reason: format!("not an asymmetric signature algorithm: {id}"),
        })
    }
}

/// 按文本编码约定构造 [`KeyRef`]（PEM 或 Base64 原始字节）。
///
/// # Errors
///
/// 文本为空或 Base64 非法时返回 [`CryptoError::InvalidKey`]。
pub fn key_ref_from_text(purpose: KeyPurpose, text: &str) -> Result<KeyRef, CryptoError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(CryptoError::InvalidKey {
            reason: "empty key text".to_owned(),
        });
    }
    if trimmed.starts_with("-----BEGIN") {
        return Ok(KeyRef::pem(purpose, trimmed.to_owned()));
    }
    let bytes = STANDARD
        .decode(trimmed.as_bytes())
        .map_err(|_| CryptoError::InvalidKey {
            reason: "key text is neither PEM nor standard base64".to_owned(),
        })?;
    Ok(KeyRef::raw(purpose, bytes))
}

/// 返回闭集签名算法的分派句柄（`sm2-sign` 使用默认 `user_id`）。
///
/// # Errors
///
/// 非签名算法时返回 [`CryptoError::Unsupported`]。
pub fn asym_signer(id: AlgorithmId) -> Result<Box<dyn AsymSign>, CryptoError> {
    ensure_sign_alg(id)?;
    if id == AlgorithmId::Sm2Sign {
        return Ok(Box::new(Sm2SignAlgorithm::default()));
    }
    Ok(Box::new(AsymSignAlgorithm::from_id(id)?))
}

/// 校验公钥文本可被该签名算法解析（RSA 另要求模长 ≥ [`RSA_SIGN_MIN_BITS`]）。
///
/// # Errors
///
/// 非签名算法、格式非法、曲线不符或 RSA 模长不足时返回错误。
pub fn validate_sign_public_key(id: AlgorithmId, public_text: &str) -> Result<(), CryptoError> {
    ensure_sign_alg(id)?;
    let key = key_ref_from_text(KeyPurpose::AsymSign, public_text)?;
    match id {
        AlgorithmId::RsaPkcs1v15SignSha256
        | AlgorithmId::RsaPkcs1v15SignSha384
        | AlgorithmId::RsaPkcs1v15SignSha512 => {
            let public = parse_rsa_public(&key)?;
            let bits = public.size() * 8;
            if bits < RSA_SIGN_MIN_BITS {
                return Err(CryptoError::InvalidKey {
                    reason: format!(
                        "RSA public key must be at least {RSA_SIGN_MIN_BITS} bits, got {bits}"
                    ),
                });
            }
            Ok(())
        }
        AlgorithmId::EcdsaP256Sha256 => parse_p256_verifying(&key).map(drop),
        AlgorithmId::EcdsaP384Sha384 => parse_p384_verifying(&key).map(drop),
        AlgorithmId::Ed25519 => parse_ed25519_verifying(&key).map(drop),
        AlgorithmId::Sm2Sign => parse_sm2_public(&key).map(drop),
        other => Err(CryptoError::Unsupported {
            reason: format!("not an implemented asym sign algorithm: {other}"),
        }),
    }
}

fn pem_error(what: &str) -> CryptoError {
    CryptoError::OperationFailed {
        reason: format!("failed to encode {what} PEM"),
    }
}

/// 生成签名密钥对（统一 PEM：私钥 PKCS#8、公钥 SPKI）。
///
/// # Errors
///
/// 非签名算法或底层生成 / 编码失败时返回错误。
pub fn generate_sign_keypair(id: AlgorithmId) -> Result<SignKeyPair, CryptoError> {
    use pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
    ensure_sign_alg(id)?;
    let (private_pem, public_pem) = match id {
        AlgorithmId::RsaPkcs1v15SignSha256
        | AlgorithmId::RsaPkcs1v15SignSha384
        | AlgorithmId::RsaPkcs1v15SignSha512 => {
            let private = rsa::RsaPrivateKey::new(&mut OsRng, RSA_SIGN_KEY_BITS).map_err(|e| {
                CryptoError::OperationFailed {
                    reason: format!("rsa keygen: {e}"),
                }
            })?;
            let public = rsa::RsaPublicKey::from(&private);
            (
                private
                    .to_pkcs8_pem(LineEnding::LF)
                    .map_err(|_| pem_error("RSA private"))?,
                public
                    .to_public_key_pem(LineEnding::LF)
                    .map_err(|_| pem_error("RSA public"))?,
            )
        }
        AlgorithmId::EcdsaP256Sha256 => {
            let secret = p256::SecretKey::random(&mut OsRng);
            (
                secret
                    .to_pkcs8_pem(LineEnding::LF)
                    .map_err(|_| pem_error("P-256 private"))?,
                secret
                    .public_key()
                    .to_public_key_pem(LineEnding::LF)
                    .map_err(|_| pem_error("P-256 public"))?,
            )
        }
        AlgorithmId::EcdsaP384Sha384 => {
            let secret = p384::SecretKey::random(&mut OsRng);
            (
                secret
                    .to_pkcs8_pem(LineEnding::LF)
                    .map_err(|_| pem_error("P-384 private"))?,
                secret
                    .public_key()
                    .to_public_key_pem(LineEnding::LF)
                    .map_err(|_| pem_error("P-384 public"))?,
            )
        }
        AlgorithmId::Ed25519 => {
            use ed25519_dalek::pkcs8::{EncodePrivateKey as _, EncodePublicKey as _};
            let mut seed = Zeroizing::new([0_u8; 32]);
            OsRng.fill_bytes(seed.as_mut());
            let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
            let private = signing
                .to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
                .map_err(|_| pem_error("Ed25519 private"))?;
            let public = signing
                .verifying_key()
                .to_public_key_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
                .map_err(|_| pem_error("Ed25519 public"))?;
            (Zeroizing::new(private.to_string()), public)
        }
        AlgorithmId::Sm2Sign => {
            let ctx = libsm::sm2::signature::SigCtx::new();
            let (pk, sk) = ctx
                .new_keypair()
                .map_err(|e| CryptoError::OperationFailed {
                    reason: format!("sm2 keygen: {e}"),
                })?;
            let scalar = Zeroizing::new(ctx.serialize_seckey(&sk).map_err(|e| {
                CryptoError::OperationFailed {
                    reason: format!("sm2 serialize private: {e}"),
                }
            })?);
            let point =
                ctx.serialize_pubkey(&pk, false)
                    .map_err(|e| CryptoError::OperationFailed {
                        reason: format!("sm2 serialize public: {e}"),
                    })?;
            (
                sm2_private_key_to_pem(&scalar)?,
                sm2_public_key_to_pem(&point)?,
            )
        }
        other => {
            return Err(CryptoError::Unsupported {
                reason: format!("not an implemented asym sign algorithm: {other}"),
            });
        }
    };
    Ok(SignKeyPair {
        alg: id,
        private_pem: Zeroizing::new(private_pem.to_string()),
        public_pem,
    })
}
