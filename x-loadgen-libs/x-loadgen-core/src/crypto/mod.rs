//! 密码学适配层：传输信封与请求绑定走 [`x_loadgen_crypto`]，业务侧不直连 AEAD 实现。

pub mod request_binding;
pub mod transport;

pub use transport::default_suite;
pub use x_loadgen_crypto::profile::{
    BindingSpec, DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS, KeySelection, PROFILE_VERSION_V1,
    TenantTransportProfile, TransportMode,
};
pub use x_loadgen_crypto::registry::{AlgorithmRegistry, parse_algorithm_id};
pub use x_loadgen_crypto::suite::{
    SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20, SUITE_TENANT_DEFAULT_2026_11, known_suites,
    resolve_suite,
};
pub use x_loadgen_crypto::{
    AlgorithmId, AlgorithmKind, CryptoError, KeyRef, SignKeyPair, generate_sign_keypair,
    key_ref_from_text, validate_sign_public_key,
};
