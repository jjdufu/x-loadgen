//! x-loadgen 的软件密码学原语与算法闭集（无 HSM / 密码机）。
//!
//! 只保留 mazbri 网关客户端侧需要的部分，线上格式（算法 id、套件定义）与 mazbri 平台一致：
//!
//! - 闭集 [`AlgorithmId`] 与 [`SecurityClass`]，kebab-case 精确解析
//! - 原语 traits：[`AeadSeal`]、[`BlockCipher`]、[`Mac`]、[`AsymSign`]
//! - [`AlgorithmRegistry`]：按 id 取实现句柄，业务侧不散落算法常量
//! - 对称：AES-GCM / ChaCha20-Poly1305 / AES-CBC·CTR·CFB / SM4-CBC·CTR
//! - MAC：HMAC-SHA256/384/512、HMAC-SM3
//! - 签名：RSA PKCS#1 v1.5、ECDSA P-256/P-384、Ed25519、SM2，以及签名密钥对生成
//! - [`profile::TenantTransportProfile`] 与命名套件解析
//! - Legacy 算法（ECB / DES / HMAC-MD5）唯一门闩：[`legacy::assert_not_legacy_or_allowed`]
//!
//! # 非目标
//!
//! - 不做 SM9（[`registry::parse_algorithm_id`] 对 `sm9-*` 返回 [`CryptoError::Unsupported`]）
//! - 不做假 `sm4-gcm`；国密 AEAD 用 [`suite::SUITE_GM_SM4_HMAC_SM3`]
//! - 不含非对称加密 / 密钥包装与上游加密画像（客户端用不到）

#![forbid(unsafe_code)]

pub mod aead;
pub mod asym;
pub mod id;
pub mod legacy;
pub mod legacy_algs;
pub mod mac;
pub mod material;
pub mod profile;
pub mod registry;
pub mod security;
pub mod signing_key;
pub mod sm;
pub mod suite;
pub mod symmetric;
pub mod traits;

pub use aead::AeadAlgorithm;
pub use asym::AsymSignAlgorithm;
pub use id::{AlgorithmId, AlgorithmKind};
pub use mac::MacAlgorithm;
pub use material::{KeyPurpose, KeyRef, NoncePolicy, ZeroizingBytes};
pub use profile::{
    BindingSpec, DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS, KeySelection, PROFILE_VERSION_V1,
    TenantTransportProfile, TransportMode,
};
pub use registry::{AlgorithmMeta, AlgorithmRegistry, parse_algorithm_id, require_mac_for};
pub use security::SecurityClass;
pub use signing_key::{
    RSA_SIGN_KEY_BITS, RSA_SIGN_MIN_BITS, SignKeyPair, asym_signer, generate_sign_keypair,
    key_ref_from_text, validate_sign_public_key,
};
pub use sm::{
    SM2_DEFAULT_USER_ID, SM3_OUTPUT_LEN, SM4_BLOCK_LEN, SM4_KEY_LEN, Sm2SignAlgorithm,
    Sm4Algorithm, sm2_private_key_to_pem, sm2_public_key_to_pem, sm4_ecb_decrypt, sm4_ecb_encrypt,
};
pub use suite::{
    SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20, SUITE_TENANT_DEFAULT_2026_11, SuiteSpec,
    known_suites, resolve_suite, suite_data_alg, suite_mac_alg,
};
pub use symmetric::{CfbParams, SymmetricAlgorithm, decrypt_cfb, encrypt_cfb};
pub use traits::{AeadSeal, AsymSign, BlockCipher, CryptoError, Mac};
