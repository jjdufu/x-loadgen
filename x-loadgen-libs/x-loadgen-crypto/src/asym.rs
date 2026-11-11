//! 主流非对称签名原语：RSA PKCS#1 v1.5、ECDSA P-256/P-384、Ed25519。
//!
//! 密钥材料：
//! - RSA：[`KeyRef::Pem`]（PKCS#1 / PKCS#8 / SPKI）
//! - ECDSA：[`KeyRef::Pem`] 或 [`KeyRef::Raw`]（私钥标量 / SEC1 公钥）
//! - Ed25519：[`KeyRef::Pem`] 或 [`KeyRef::Raw`]（32 字节种子 / 32 字节公钥）

use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};
use p256::SecretKey as P256SecretKey;
use p256::ecdsa::signature::{Signer as EcdsaSigner, Verifier as EcdsaVerifier};
use p256::ecdsa::{
    Signature as P256Signature, SigningKey as P256SigningKey, VerifyingKey as P256VerifyingKey,
};
use p384::SecretKey as P384SecretKey;
use p384::ecdsa::{
    Signature as P384Signature, SigningKey as P384SigningKey, VerifyingKey as P384VerifyingKey,
};
use rsa::pkcs1v15::{SigningKey as RsaSigningKey, VerifyingKey as RsaVerifyingKey};
use rsa::sha2::{Sha256, Sha384, Sha512};
use rsa::signature::{SignatureEncoding, Signer as RsaSigner, Verifier as RsaVerifier};
use rsa::{RsaPrivateKey, RsaPublicKey};

use crate::id::AlgorithmId;
use crate::material::KeyRef;
use crate::traits::{AsymSign, CryptoError};

/// 本步已实现的非对称签名算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsymSignAlgorithm {
    /// `rsa-pkcs1v15-sign-sha256`
    RsaPkcs1v15SignSha256,
    /// `rsa-pkcs1v15-sign-sha384`
    RsaPkcs1v15SignSha384,
    /// `rsa-pkcs1v15-sign-sha512`
    RsaPkcs1v15SignSha512,
    /// `ecdsa-p256-sha256`
    EcdsaP256Sha256,
    /// `ecdsa-p384-sha384`
    EcdsaP384Sha384,
    /// `ed25519`
    Ed25519,
}

impl AsymSignAlgorithm {
    /// 由闭集 id 解析。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非本模块签名算法（含 SM2）。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::RsaPkcs1v15SignSha256 => Ok(Self::RsaPkcs1v15SignSha256),
            AlgorithmId::RsaPkcs1v15SignSha384 => Ok(Self::RsaPkcs1v15SignSha384),
            AlgorithmId::RsaPkcs1v15SignSha512 => Ok(Self::RsaPkcs1v15SignSha512),
            AlgorithmId::EcdsaP256Sha256 => Ok(Self::EcdsaP256Sha256),
            AlgorithmId::EcdsaP384Sha384 => Ok(Self::EcdsaP384Sha384),
            AlgorithmId::Ed25519 => Ok(Self::Ed25519),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented asym sign algorithm: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::RsaPkcs1v15SignSha256 => AlgorithmId::RsaPkcs1v15SignSha256,
            Self::RsaPkcs1v15SignSha384 => AlgorithmId::RsaPkcs1v15SignSha384,
            Self::RsaPkcs1v15SignSha512 => AlgorithmId::RsaPkcs1v15SignSha512,
            Self::EcdsaP256Sha256 => AlgorithmId::EcdsaP256Sha256,
            Self::EcdsaP384Sha384 => AlgorithmId::EcdsaP384Sha384,
            Self::Ed25519 => AlgorithmId::Ed25519,
        }
    }
}

pub(crate) fn parse_rsa_public(key: &KeyRef) -> Result<RsaPublicKey, CryptoError> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;
    let pem = key.pem_str()?;
    if let Ok(pk) = RsaPublicKey::from_public_key_pem(pem) {
        return Ok(pk);
    }
    RsaPublicKey::from_pkcs1_pem(pem).map_err(|_| CryptoError::InvalidKey {
        reason: "failed to parse RSA public PEM (SPKI/PKCS#1)".to_owned(),
    })
}

pub(crate) fn parse_rsa_private(key: &KeyRef) -> Result<RsaPrivateKey, CryptoError> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    let pem = key.pem_str()?;
    if let Ok(sk) = RsaPrivateKey::from_pkcs8_pem(pem) {
        return Ok(sk);
    }
    RsaPrivateKey::from_pkcs1_pem(pem).map_err(|_| CryptoError::InvalidKey {
        reason: "failed to parse RSA private PEM (PKCS#8/PKCS#1)".to_owned(),
    })
}

pub(crate) fn parse_p256_signing(key: &KeyRef) -> Result<P256SigningKey, CryptoError> {
    match key {
        KeyRef::Pem { pem, .. } => {
            use p256::pkcs8::DecodePrivateKey;
            let secret =
                P256SecretKey::from_pkcs8_pem(pem).map_err(|_| CryptoError::InvalidKey {
                    reason: "failed to parse P-256 private PEM".to_owned(),
                })?;
            Ok(P256SigningKey::from(secret))
        }
        KeyRef::Raw { bytes, .. } => {
            let slice = bytes.as_slice();
            let arr: [u8; 32] = slice.try_into().map_err(|_| CryptoError::InvalidKey {
                reason: format!(
                    "P-256 raw private key must be 32 bytes, got {}",
                    slice.len()
                ),
            })?;
            P256SigningKey::from_bytes((&arr).into()).map_err(|_| CryptoError::InvalidKey {
                reason: "invalid P-256 raw private scalar".to_owned(),
            })
        }
    }
}

pub(crate) fn parse_p256_verifying(key: &KeyRef) -> Result<P256VerifyingKey, CryptoError> {
    match key {
        KeyRef::Pem { pem, .. } => {
            use p256::pkcs8::DecodePublicKey;
            P256VerifyingKey::from_public_key_pem(pem).map_err(|_| CryptoError::InvalidKey {
                reason: "failed to parse P-256 public PEM".to_owned(),
            })
        }
        KeyRef::Raw { bytes, .. } => {
            use p256::elliptic_curve::sec1::FromEncodedPoint;
            use p256::{EncodedPoint, PublicKey};
            let point = EncodedPoint::from_bytes(bytes.as_slice()).map_err(|_| {
                CryptoError::InvalidKey {
                    reason: "invalid P-256 SEC1 public key bytes".to_owned(),
                }
            })?;
            let public = PublicKey::from_encoded_point(&point)
                .into_option()
                .ok_or_else(|| CryptoError::InvalidKey {
                    reason: "P-256 public point not on curve".to_owned(),
                })?;
            Ok(P256VerifyingKey::from(public))
        }
    }
}

pub(crate) fn parse_p384_signing(key: &KeyRef) -> Result<P384SigningKey, CryptoError> {
    match key {
        KeyRef::Pem { pem, .. } => {
            use p384::pkcs8::DecodePrivateKey;
            let secret =
                P384SecretKey::from_pkcs8_pem(pem).map_err(|_| CryptoError::InvalidKey {
                    reason: "failed to parse P-384 private PEM".to_owned(),
                })?;
            Ok(P384SigningKey::from(secret))
        }
        KeyRef::Raw { bytes, .. } => {
            let slice = bytes.as_slice();
            if slice.len() != 48 {
                return Err(CryptoError::InvalidKey {
                    reason: format!(
                        "P-384 raw private key must be 48 bytes, got {}",
                        slice.len()
                    ),
                });
            }
            let mut arr = [0_u8; 48];
            arr.copy_from_slice(slice);
            P384SigningKey::from_bytes((&arr).into()).map_err(|_| CryptoError::InvalidKey {
                reason: "invalid P-384 raw private scalar".to_owned(),
            })
        }
    }
}

pub(crate) fn parse_p384_verifying(key: &KeyRef) -> Result<P384VerifyingKey, CryptoError> {
    match key {
        KeyRef::Pem { pem, .. } => {
            use p384::pkcs8::DecodePublicKey;
            P384VerifyingKey::from_public_key_pem(pem).map_err(|_| CryptoError::InvalidKey {
                reason: "failed to parse P-384 public PEM".to_owned(),
            })
        }
        KeyRef::Raw { bytes, .. } => {
            use p384::elliptic_curve::sec1::FromEncodedPoint;
            use p384::{EncodedPoint, PublicKey};
            let point = EncodedPoint::from_bytes(bytes.as_slice()).map_err(|_| {
                CryptoError::InvalidKey {
                    reason: "invalid P-384 SEC1 public key bytes".to_owned(),
                }
            })?;
            let public = PublicKey::from_encoded_point(&point)
                .into_option()
                .ok_or_else(|| CryptoError::InvalidKey {
                    reason: "P-384 public point not on curve".to_owned(),
                })?;
            Ok(P384VerifyingKey::from(public))
        }
    }
}

pub(crate) fn parse_ed25519_signing(key: &KeyRef) -> Result<SigningKey, CryptoError> {
    match key {
        KeyRef::Pem { pem, .. } => {
            use ed25519_dalek::pkcs8::DecodePrivateKey;
            SigningKey::from_pkcs8_pem(pem).map_err(|_| CryptoError::InvalidKey {
                reason: "failed to parse Ed25519 private PEM".to_owned(),
            })
        }
        KeyRef::Raw { bytes, .. } => {
            let arr: [u8; 32] =
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| CryptoError::InvalidKey {
                        reason: format!(
                            "Ed25519 raw private seed must be 32 bytes, got {}",
                            bytes.as_slice().len()
                        ),
                    })?;
            Ok(SigningKey::from_bytes(&arr))
        }
    }
}

pub(crate) fn parse_ed25519_verifying(key: &KeyRef) -> Result<VerifyingKey, CryptoError> {
    match key {
        KeyRef::Pem { pem, .. } => {
            use ed25519_dalek::pkcs8::DecodePublicKey;
            VerifyingKey::from_public_key_pem(pem).map_err(|_| CryptoError::InvalidKey {
                reason: "failed to parse Ed25519 public PEM".to_owned(),
            })
        }
        KeyRef::Raw { bytes, .. } => {
            let arr: [u8; 32] =
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| CryptoError::InvalidKey {
                        reason: format!(
                            "Ed25519 raw public key must be 32 bytes, got {}",
                            bytes.as_slice().len()
                        ),
                    })?;
            VerifyingKey::from_bytes(&arr).map_err(|_| CryptoError::InvalidKey {
                reason: "invalid Ed25519 public key bytes".to_owned(),
            })
        }
    }
}

impl AsymSign for AsymSignAlgorithm {
    fn sign(&self, signer: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match self {
            Self::RsaPkcs1v15SignSha256 => {
                let private = parse_rsa_private(signer)?;
                let signing = RsaSigningKey::<Sha256>::new(private);
                Ok(RsaSigner::sign(&signing, message).to_bytes().into_vec())
            }
            Self::RsaPkcs1v15SignSha384 => {
                let private = parse_rsa_private(signer)?;
                let signing = RsaSigningKey::<Sha384>::new(private);
                Ok(RsaSigner::sign(&signing, message).to_bytes().into_vec())
            }
            Self::RsaPkcs1v15SignSha512 => {
                let private = parse_rsa_private(signer)?;
                let signing = RsaSigningKey::<Sha512>::new(private);
                Ok(RsaSigner::sign(&signing, message).to_bytes().into_vec())
            }
            Self::EcdsaP256Sha256 => {
                let signing = parse_p256_signing(signer)?;
                let sig: P256Signature = EcdsaSigner::sign(&signing, message);
                Ok(sig.to_bytes().to_vec())
            }
            Self::EcdsaP384Sha384 => {
                let signing = parse_p384_signing(signer)?;
                let sig: P384Signature = EcdsaSigner::sign(&signing, message);
                Ok(sig.to_bytes().to_vec())
            }
            Self::Ed25519 => {
                let signing = parse_ed25519_signing(signer)?;
                let sig: Ed25519Signature = Signer::sign(&signing, message);
                Ok(sig.to_bytes().to_vec())
            }
        }
    }

    fn verify(
        &self,
        verifier: &KeyRef,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), CryptoError> {
        match self {
            Self::RsaPkcs1v15SignSha256 => {
                let public = parse_rsa_public(verifier)?;
                let verifying = RsaVerifyingKey::<Sha256>::new(public);
                let sig = rsa::pkcs1v15::Signature::try_from(signature).map_err(|_| {
                    CryptoError::InvalidInput {
                        reason: "invalid RSA PKCS1v15 signature encoding".to_owned(),
                    }
                })?;
                RsaVerifier::verify(&verifying, message, &sig)
                    .map_err(|_| CryptoError::AuthenticationFailed)
            }
            Self::RsaPkcs1v15SignSha384 => {
                let public = parse_rsa_public(verifier)?;
                let verifying = RsaVerifyingKey::<Sha384>::new(public);
                let sig = rsa::pkcs1v15::Signature::try_from(signature).map_err(|_| {
                    CryptoError::InvalidInput {
                        reason: "invalid RSA PKCS1v15 signature encoding".to_owned(),
                    }
                })?;
                RsaVerifier::verify(&verifying, message, &sig)
                    .map_err(|_| CryptoError::AuthenticationFailed)
            }
            Self::RsaPkcs1v15SignSha512 => {
                let public = parse_rsa_public(verifier)?;
                let verifying = RsaVerifyingKey::<Sha512>::new(public);
                let sig = rsa::pkcs1v15::Signature::try_from(signature).map_err(|_| {
                    CryptoError::InvalidInput {
                        reason: "invalid RSA PKCS1v15 signature encoding".to_owned(),
                    }
                })?;
                RsaVerifier::verify(&verifying, message, &sig)
                    .map_err(|_| CryptoError::AuthenticationFailed)
            }
            Self::EcdsaP256Sha256 => {
                let verifying = parse_p256_verifying(verifier)?;
                let sig = P256Signature::from_slice(signature).map_err(|_| {
                    CryptoError::InvalidInput {
                        reason: "invalid ECDSA P-256 signature encoding".to_owned(),
                    }
                })?;
                EcdsaVerifier::verify(&verifying, message, &sig)
                    .map_err(|_| CryptoError::AuthenticationFailed)
            }
            Self::EcdsaP384Sha384 => {
                let verifying = parse_p384_verifying(verifier)?;
                let sig = P384Signature::from_slice(signature).map_err(|_| {
                    CryptoError::InvalidInput {
                        reason: "invalid ECDSA P-384 signature encoding".to_owned(),
                    }
                })?;
                EcdsaVerifier::verify(&verifying, message, &sig)
                    .map_err(|_| CryptoError::AuthenticationFailed)
            }
            Self::Ed25519 => {
                let verifying = parse_ed25519_verifying(verifier)?;
                let sig = Ed25519Signature::from_slice(signature).map_err(|_| {
                    CryptoError::InvalidInput {
                        reason: "invalid Ed25519 signature encoding".to_owned(),
                    }
                })?;
                Verifier::verify(&verifying, message, &sig)
                    .map_err(|_| CryptoError::AuthenticationFailed)
            }
        }
    }
}
