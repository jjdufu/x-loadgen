//! 空注册表 / 未知 id / legacy 门闩冒烟测试。

use x_loadgen_crypto::legacy::assert_not_legacy_or_allowed;
use x_loadgen_crypto::{
    AeadSeal, AlgorithmId, AlgorithmMeta, AlgorithmRegistry, AsymSign, BlockCipher, CryptoError,
    KeyPurpose, KeyRef, Mac, NoncePolicy, SecurityClass,
};

#[test]
fn empty_registry_get_returns_not_registered() {
    let registry = AlgorithmRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    let err = registry
        .get(AlgorithmId::Aes256Gcm)
        .expect_err("empty registry must not resolve");
    assert_eq!(
        err,
        CryptoError::NotRegistered {
            id: AlgorithmId::Aes256Gcm
        }
    );
}

#[test]
fn register_then_get_and_list() {
    let mut registry = AlgorithmRegistry::new();
    registry.register_id(AlgorithmId::Aes256Gcm);
    registry.register(AlgorithmMeta::from_id(AlgorithmId::HmacSha256));

    let meta = registry.get(AlgorithmId::Aes256Gcm).expect("registered");
    assert_eq!(meta.security, SecurityClass::Recommended);
    assert!(registry.contains(AlgorithmId::HmacSha256));
    assert!(!registry.contains(AlgorithmId::Sm4CbcPkcs7));

    let ids = registry.registered_ids();
    assert_eq!(ids.len(), 2);
    assert_eq!(AlgorithmRegistry::closed_set(), AlgorithmId::all());
    assert_ne!(AlgorithmRegistry::closed_set().len(), 0);
}

#[test]
fn unknown_algorithm_id_from_str() {
    let err = "not-an-alg".parse::<AlgorithmId>().expect_err("unknown id");
    assert!(matches!(
        err,
        CryptoError::UnknownAlgorithmId { raw } if raw == "not-an-alg"
    ));
}

#[test]
fn legacy_gate_rejects_without_allow_insecure() {
    let id = AlgorithmId::Aes256Ecb;
    assert_eq!(id.security_class(), SecurityClass::Legacy);
    assert!(id.security_class().requires_allow_insecure());

    let err = assert_not_legacy_or_allowed(id, false).expect_err("must reject");
    assert_eq!(err, CryptoError::LegacyNotAllowed { id });

    assert_not_legacy_or_allowed(id, true).expect("explicit allow");
    assert_not_legacy_or_allowed(AlgorithmId::Aes256Gcm, false).expect("non-legacy ok");
}

#[test]
fn key_ref_and_nonce_policy_construct() {
    let key = KeyRef::raw(KeyPurpose::DataEncryption, vec![0_u8; 32]);
    assert_eq!(key.purpose(), KeyPurpose::DataEncryption);
    let _ = format!("{key:?}");
    let pem = KeyRef::pem(KeyPurpose::AsymSign, "-----BEGIN PUBLIC KEY-----\n");
    assert_eq!(pem.purpose(), KeyPurpose::AsymSign);
    assert_eq!(NoncePolicy::Random, NoncePolicy::Random);
}

/// 仅保证 traits 在类型系统中可作 bound（本步无具体实现）。
fn _traits_are_object_safe(
    _aead: &dyn AeadSeal,
    _block: &dyn BlockCipher,
    _mac: &dyn Mac,
    _sign: &dyn AsymSign,
) {
}

#[test]
fn traits_object_safe_symbol_linked() {
    // 引用私有函数符号，避免 dead_code；运行时不构造 dyn。
    let _ = _traits_are_object_safe as fn(&dyn AeadSeal, &dyn BlockCipher, &dyn Mac, &dyn AsymSign);
}
