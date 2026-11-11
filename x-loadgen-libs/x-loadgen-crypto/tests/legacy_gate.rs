//! Legacy 门闩：未设 `allow_insecure` 时入口失败；设置后可运算。

use x_loadgen_crypto::legacy_algs::{
    aes128_ecb_decrypt, aes128_ecb_encrypt, aes256_ecb_decrypt, aes256_ecb_encrypt,
    des_cbc_decrypt, des_cbc_encrypt, hmac_md5_sign, hmac_md5_verify,
};
use x_loadgen_crypto::{
    AlgorithmId, CryptoError, KeyPurpose, KeyRef, sm4_ecb_decrypt, sm4_ecb_encrypt,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

#[test]
fn legacy_denied_without_allow() {
    let key16 = KeyRef::raw(
        KeyPurpose::DataEncryption,
        unhex("0123456789abcdeffedcba9876543210"),
    );
    let err = aes128_ecb_encrypt(false, &key16, b"HelloSM4!").expect_err("deny");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes128Ecb
        }
    ));

    let mac_key = KeyRef::raw(KeyPurpose::Mac, b"key".to_vec());
    let err = hmac_md5_sign(false, &mac_key, b"abc").expect_err("deny hmac-md5");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::HmacMd5
        }
    ));

    let err = sm4_ecb_encrypt(false, &key16, b"HelloSM4!").expect_err("deny sm4ecb");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Sm4EcbPkcs7
        }
    ));
}

#[test]
fn aes_ecb_openssl_vectors_when_allowed() {
    let key16 = KeyRef::raw(
        KeyPurpose::DataEncryption,
        unhex("0123456789abcdeffedcba9876543210"),
    );
    let key32 = KeyRef::raw(
        KeyPurpose::DataEncryption,
        unhex("0123456789abcdeffedcba98765432100123456789abcdeffedcba9876543210"),
    );
    let pt = b"HelloSM4!";

    let c128 = aes128_ecb_encrypt(true, &key16, pt).expect("enc");
    assert_eq!(c128, unhex("6f106b356be6db64a5e4c85dd3402aa2"));
    assert_eq!(aes128_ecb_decrypt(true, &key16, &c128).expect("dec"), pt);

    let c256 = aes256_ecb_encrypt(true, &key32, pt).expect("enc");
    assert_eq!(c256, unhex("4bf5e3dade3f9875d6268560adae9493"));
    assert_eq!(aes256_ecb_decrypt(true, &key32, &c256).expect("dec"), pt);
}

#[test]
fn sm4_ecb_and_des_cbc_when_allowed() {
    let key16 = KeyRef::raw(
        KeyPurpose::DataEncryption,
        unhex("0123456789abcdeffedcba9876543210"),
    );
    let pt = b"HelloSM4!";
    let c = sm4_ecb_encrypt(true, &key16, pt).expect("sm4ecb");
    assert_eq!(c, unhex("50763de677717cd8bf34e9f226143bd4"));
    assert_eq!(sm4_ecb_decrypt(true, &key16, &c).expect("dec"), pt);

    let desk = KeyRef::raw(KeyPurpose::DataEncryption, unhex("0123456789abcdef"));
    let iv = unhex("0001020304050607");
    let cdes = des_cbc_encrypt(true, &desk, &iv, pt).expect("des");
    assert_eq!(cdes, unhex("b613ef5ee705ab44e8ae60904fee34c6"));
    assert_eq!(des_cbc_decrypt(true, &desk, &iv, &cdes).expect("dec"), pt);
}

#[test]
fn hmac_md5_when_allowed() {
    let key = KeyRef::raw(KeyPurpose::Mac, b"key".to_vec());
    let tag = hmac_md5_sign(true, &key, b"abc").expect("hmac");
    assert_eq!(tag, unhex("d2fe98063f876b03193afb49b4979591"));
    hmac_md5_verify(true, &key, b"abc", &tag).expect("verify");
}
