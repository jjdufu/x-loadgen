//! 密码学适配层：传输信封走 [`x_loadgen_crypto`]，业务侧不直连 AEAD 实现。

pub mod transport;

pub use transport::default_suite;
pub use x_loadgen_crypto::profile::{
    DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS, TenantTransportProfile, TransportMode,
};
pub use x_loadgen_crypto::registry::{AlgorithmRegistry, parse_algorithm_id};
pub use x_loadgen_crypto::suite::{
    SUITE_AES_256_GCM, SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20, known_suites, resolve_suite,
};
pub use x_loadgen_crypto::traits::Mac;
pub use x_loadgen_crypto::{
    AlgorithmId, AlgorithmKind, CryptoError, KeyPurpose, KeyRef, SignKeyPair,
    generate_sign_keypair, key_ref_from_text, validate_sign_public_key,
};
