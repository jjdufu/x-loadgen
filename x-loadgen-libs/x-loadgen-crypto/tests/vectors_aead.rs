//! AEAD 互操作向量（NIST / RFC 8439）与注册表 seal/open。

use x_loadgen_crypto::{
    AeadAlgorithm, AeadSeal, AlgorithmId, AlgorithmRegistry, CryptoError, KeyPurpose, KeyRef,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

fn data_key(bytes: &[u8]) -> KeyRef {
    KeyRef::raw(KeyPurpose::DataEncryption, bytes.to_vec())
}

#[test]
fn aes128_gcm_nist_empty() {
    let key = unhex("00000000000000000000000000000000");
    let nonce = unhex("000000000000000000000000");
    let sealed = AeadAlgorithm::Aes128Gcm
        .seal(&data_key(&key), &nonce, b"", b"")
        .expect("seal");
    assert_eq!(sealed, unhex("58e2fccefa7e3061367f1d57a4e7455a"));
    let pt = AeadAlgorithm::Aes128Gcm
        .open(&data_key(&key), &nonce, b"", &sealed)
        .expect("open");
    assert_eq!(pt, [] as [u8; 0]);
}

#[test]
fn aes256_gcm_nist_empty() {
    let key = unhex("0000000000000000000000000000000000000000000000000000000000000000");
    let nonce = unhex("000000000000000000000000");
    let sealed = AeadAlgorithm::Aes256Gcm
        .seal(&data_key(&key), &nonce, b"", b"")
        .expect("seal");
    assert_eq!(sealed, unhex("530f8afbc74536b9a963b4f1c4cb738b"));
}

#[test]
fn aes256_gcm_nist_64_via_registry() {
    let key = unhex("feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308");
    let nonce = unhex("cafebabefacedbaddecaf888");
    let aad = unhex("feedfacedeadbeeffeedfacedeadbeefabaddad2");
    let pt = unhex(
        "d9313225f88406e5a55909c5aff5269a\
         86a7a9531534f7da2e4c303d8a318a72\
         1c3c0c95956809532fcf0e2449a6b525\
         b16aedf5aa0de657ba637b391aafd255",
    );
    let expected = unhex(
        "522dc1f099567d07f47f37a32a84427d\
         643a8cdcbfe5c0c97598a2bd2555d1aa\
         8cb08e48590dbb3da7b08b1056828838\
         c5f61e6393ba7a0abcc9f662898015ad\
         2df7cd675b4f09163b41ebf980a7f638",
    );
    let registry = AlgorithmRegistry::default_registry();
    let alg = registry.aead(AlgorithmId::Aes256Gcm).expect("registered");
    let sealed = alg.seal(&data_key(&key), &nonce, &aad, &pt).expect("seal");
    assert_eq!(sealed, expected);
    let opened = alg
        .open(&data_key(&key), &nonce, &aad, &sealed)
        .expect("open");
    assert_eq!(opened, pt);
}

#[test]
fn aes192_gcm_roundtrip_via_registry() {
    let key = unhex("feffe9928665731c6d6a8f9467308308feffe9928665731c");
    let nonce = unhex("000000000000000000000000");
    let expected = unhex("93e4895e3ee9e5787be251b90b792569397725");
    let registry = AlgorithmRegistry::default_registry();
    let alg = registry.aead(AlgorithmId::Aes192Gcm).expect("registered");
    let sealed = alg
        .seal(&data_key(&key), &nonce, b"", b"abc")
        .expect("seal");
    assert_eq!(sealed, expected);
}

#[test]
fn chacha20_poly1305_rfc8439() {
    let key = unhex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
    let nonce = unhex("070000004041424344454647");
    let aad = unhex("50515253c0c1c2c3c4c5c6c7");
    let pt = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    let expected = unhex(
        "d31a8d34648e60db7b86afbc53ef7ec2\
         a4aded51296e08fea9e2b5a736ee62d6\
         3dbea45e8ca9671282fafb69da92728b\
         1a71de0a9e060b2905d6a5b67ecd3b36\
         92ddbd7f2d778b8c9803aee328091b58\
         fab324e4fad675945585808b4831d7bc\
         3ff4def08e4b7a9de576d26586cec64b\
         61161ae10b594f09e26a7e902ecbd0600691",
    );
    let sealed = AeadAlgorithm::Chacha20Poly1305
        .seal(&data_key(&key), &nonce, &aad, pt)
        .expect("seal");
    assert_eq!(sealed, expected);
    let opened = AeadAlgorithm::Chacha20Poly1305
        .open(&data_key(&key), &nonce, &aad, &sealed)
        .expect("open");
    assert_eq!(opened, pt);
}

#[test]
fn aead_rejects_bad_nonce_and_auth() {
    let key = data_key(&[0_u8; 32]);
    let err = AeadAlgorithm::Aes256Gcm
        .seal(&key, &[0_u8; 8], b"", b"hi")
        .expect_err("nonce");
    assert!(matches!(err, CryptoError::InvalidNonce { .. }));

    let nonce = [0_u8; 12];
    let sealed = AeadAlgorithm::Aes256Gcm
        .seal(&key, &nonce, b"", b"hi")
        .expect("seal");
    let mut bad = sealed;
    bad[0] ^= 0xff;
    let err = AeadAlgorithm::Aes256Gcm
        .open(&key, &nonce, b"", &bad)
        .expect_err("auth");
    assert_eq!(err, CryptoError::AuthenticationFailed);
}
