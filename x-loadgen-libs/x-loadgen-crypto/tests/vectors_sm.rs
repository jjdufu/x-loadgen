//! 国密 HMAC-SM3 / SM4 / SM2 签名向量与互通约定。
//!
//! - SM2 签名默认 `user_id` = [`SM2_DEFAULT_USER_ID`]（`1234567812345678`）
//!
//! # SM4-GCM
//!
//! 不提供；请用套件 `suite/gm-sm4-hmac-sm3`。

use x_loadgen_crypto::{
    AlgorithmId, AlgorithmRegistry, AsymSign, BlockCipher, CryptoError, KeyPurpose, KeyRef, Mac,
    MacAlgorithm, SM2_DEFAULT_USER_ID, SUITE_GM_SM4_HMAC_SM3, Sm2SignAlgorithm, Sm4Algorithm,
    parse_algorithm_id, suite_data_alg, suite_mac_alg,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

#[test]
fn hmac_sm3_openssl() {
    // key = ASCII "key", msg = "abc"；OpenSSL: dgst -sm3 -mac HMAC -macopt hexkey:6b6579
    let key = KeyRef::raw(KeyPurpose::Mac, b"key".to_vec());
    let tag = AlgorithmRegistry::default_registry()
        .mac(AlgorithmId::HmacSm3)
        .expect("reg")
        .sign(&key, b"abc")
        .expect("sign");
    assert_eq!(
        tag,
        unhex("28e63256e7c5a087b1f073265dc53092163f7b82729735d06f28f10af9d52393")
    );
    MacAlgorithm::HmacSm3
        .verify(&key, b"abc", &tag)
        .expect("verify");
}

#[test]
fn sm4_cbc_ctr_openssl() {
    let key = KeyRef::raw(
        KeyPurpose::DataEncryption,
        unhex("0123456789abcdeffedcba9876543210"),
    );
    let iv = unhex("000102030405060708090a0b0c0d0e0f");
    let pt = b"HelloSM4!";

    let cbc = Sm4Algorithm::Sm4CbcPkcs7
        .encrypt(&key, &iv, pt)
        .expect("cbc enc");
    assert_eq!(cbc, unhex("b3c4f0e74d54d7ee1c26112cee3750c9"));
    assert_eq!(
        Sm4Algorithm::Sm4CbcPkcs7
            .decrypt(&key, &iv, &cbc)
            .expect("cbc dec"),
        pt
    );

    let ctr = Sm4Algorithm::Sm4Ctr
        .encrypt(&key, &iv, pt)
        .expect("ctr enc");
    assert_eq!(ctr, unhex("4efdf00d52f525990b"));
    assert_eq!(
        Sm4Algorithm::Sm4Ctr
            .decrypt(&key, &iv, &ctr)
            .expect("ctr dec"),
        pt
    );
}

#[test]
fn sm2_sign_verify_default_user_id() {
    use libsm::sm2::signature::SigCtx;

    let ctx = SigCtx::new();
    let (pk, sk) = ctx.new_keypair().expect("keypair");
    let pk_bytes = ctx.serialize_pubkey(&pk, false).expect("pk");
    let sk_bytes = ctx.serialize_seckey(&sk).expect("sk");
    let signer = KeyRef::raw(KeyPurpose::AsymSign, sk_bytes);
    let verifier = KeyRef::raw(KeyPurpose::AsymSign, pk_bytes);
    let alg = Sm2SignAlgorithm::default();
    assert_eq!(alg.user_id, SM2_DEFAULT_USER_ID);

    let msg = b"sign-me-please";
    let sig = alg.sign(&signer, msg).expect("sign");
    alg.verify(&verifier, msg, &sig).expect("verify");
    let err = alg
        .verify(&verifier, b"tampered", &sig)
        .expect_err("tamper");
    assert!(matches!(err, CryptoError::AuthenticationFailed));
}

#[test]
fn suite_gm_sm4_hmac_sm3_mapping() {
    assert_eq!(
        suite_data_alg(SUITE_GM_SM4_HMAC_SM3).expect("data"),
        AlgorithmId::Sm4CbcPkcs7
    );
    assert_eq!(
        suite_mac_alg(SUITE_GM_SM4_HMAC_SM3).expect("mac"),
        Some(AlgorithmId::HmacSm3)
    );
    assert!(require_mac_registered());
}

fn require_mac_registered() -> bool {
    use x_loadgen_crypto::require_mac_for;
    require_mac_for(AlgorithmId::Sm4CbcPkcs7) && require_mac_for(AlgorithmId::Sm4Ctr)
}

#[test]
fn sm9_is_unsupported() {
    let err = parse_algorithm_id("sm9-encrypt").expect_err("sm9");
    assert!(matches!(err, CryptoError::Unsupported { .. }));
    let err2 = parse_algorithm_id("sm9").expect_err("sm9");
    assert!(matches!(err2, CryptoError::Unsupported { .. }));
}
