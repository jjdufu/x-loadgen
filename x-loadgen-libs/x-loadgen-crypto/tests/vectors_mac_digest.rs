//! HMAC 已知答案与注册表查询。

use x_loadgen_crypto::{
    AlgorithmId, AlgorithmRegistry, CryptoError, KeyPurpose, KeyRef, Mac, MacAlgorithm,
    require_mac_for,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

fn mac_key(bytes: &[u8]) -> KeyRef {
    KeyRef::raw(KeyPurpose::Mac, bytes.to_vec())
}

#[test]
fn hmac_sha_rfc4231_case2() {
    let key = mac_key(b"Jefe");
    let msg = b"what do ya want for nothing?";
    let registry = AlgorithmRegistry::default_registry();

    let tag256 = registry
        .mac(AlgorithmId::HmacSha256)
        .expect("mac")
        .sign(&key, msg)
        .expect("sign");
    assert_eq!(
        tag256,
        unhex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
    );

    let tag384 = MacAlgorithm::HmacSha384.sign(&key, msg).expect("sign");
    assert_eq!(
        tag384,
        unhex(
            "af45d2e376484031617f78d2b58a6b1b\
             9c7ef464f5a01b47e42ec3736322445e\
             8e2240ca5e69e2c78b3239ecfab21649"
        )
    );

    let tag512 = MacAlgorithm::HmacSha512.sign(&key, msg).expect("sign");
    assert_eq!(
        tag512,
        unhex(
            "164b7a7bfcf819e2e395fbe73b56e0a3\
             87bd64222e831fd610270cd7ea250554\
             9758bf75c05a994a6d034f65f8f0e6fd\
             caeab1a34d4a6b4b636e070a38bce737"
        )
    );

    MacAlgorithm::HmacSha256
        .verify(&key, msg, &tag256)
        .expect("verify");
    let err = MacAlgorithm::HmacSha256
        .verify(&key, msg, &tag384[..32])
        .expect_err("bad tag");
    assert!(matches!(
        err,
        x_loadgen_crypto::CryptoError::AuthenticationFailed
    ));
}

#[test]
fn suite_tenant_default_maps_aes256_gcm() {
    assert_eq!(
        x_loadgen_crypto::suite_data_alg(x_loadgen_crypto::SUITE_TENANT_DEFAULT_2026_11)
            .expect("suite"),
        AlgorithmId::Aes256Gcm
    );
    let err = x_loadgen_crypto::suite_data_alg("suite/unknown").expect_err("unknown");
    assert!(matches!(err, CryptoError::UnknownSuite { .. }));
}

#[test]
fn aead_and_mac_do_not_require_external_mac() {
    assert!(!require_mac_for(AlgorithmId::Aes256Gcm));
    assert!(!require_mac_for(AlgorithmId::HmacSha256));
    assert!(require_mac_for(AlgorithmId::Aes256Cbc));
}
