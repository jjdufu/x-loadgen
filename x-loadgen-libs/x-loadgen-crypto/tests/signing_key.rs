//! 签名密钥对：生成 / 文本编码 / 公钥校验 / OpenSSL 互通。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use x_loadgen_crypto::{
    AlgorithmId, CryptoError, KeyPurpose, KeyRef, asym_signer, generate_sign_keypair,
    key_ref_from_text, sm2_private_key_to_pem, sm2_public_key_to_pem, validate_sign_public_key,
};

const SIGN_ALGS: [AlgorithmId; 7] = [
    AlgorithmId::RsaPkcs1v15SignSha256,
    AlgorithmId::RsaPkcs1v15SignSha384,
    AlgorithmId::RsaPkcs1v15SignSha512,
    AlgorithmId::EcdsaP256Sha256,
    AlgorithmId::EcdsaP384Sha384,
    AlgorithmId::Ed25519,
    AlgorithmId::Sm2Sign,
];

fn text_key(text: &str) -> KeyRef {
    key_ref_from_text(KeyPurpose::AsymSign, text).expect("key text")
}

#[test]
fn generated_keypairs_sign_and_verify_for_every_alg() {
    let msg = b"POST\n/api/v1/gateway\n1700000000\nnonce-0123456789abcdef\n{}";
    for alg in SIGN_ALGS {
        let pair = generate_sign_keypair(alg).expect("keygen");
        assert!(
            pair.private_pem.starts_with("-----BEGIN PRIVATE KEY-----"),
            "{alg}"
        );
        assert!(
            pair.public_pem.starts_with("-----BEGIN PUBLIC KEY-----"),
            "{alg}"
        );
        validate_sign_public_key(alg, &pair.public_pem).expect("public valid");
        let signer = asym_signer(alg).expect("signer");
        let sig = signer
            .sign(&text_key(&pair.private_pem), msg)
            .expect("sign");
        signer
            .verify(&text_key(&pair.public_pem), msg, &sig)
            .unwrap_or_else(|e| panic!("{alg} verify: {e}"));
        assert!(
            signer
                .verify(&text_key(&pair.public_pem), b"tampered", &sig)
                .is_err(),
            "{alg} must reject tampered message"
        );
        let other = generate_sign_keypair(alg).expect("keygen other");
        assert!(
            signer
                .verify(&text_key(&other.public_pem), msg, &sig)
                .is_err(),
            "{alg} must reject other public key"
        );
        assert!(
            !format!("{pair:?}").contains("PRIVATE KEY"),
            "debug must redact"
        );
    }
}

#[test]
fn public_validation_rejects_private_pem_wrong_curve_and_non_sign_alg() {
    let p256 = generate_sign_keypair(AlgorithmId::EcdsaP256Sha256).expect("keygen");
    let sm2 = generate_sign_keypair(AlgorithmId::Sm2Sign).expect("keygen");
    assert!(validate_sign_public_key(AlgorithmId::EcdsaP256Sha256, &p256.private_pem).is_err());
    assert!(validate_sign_public_key(AlgorithmId::EcdsaP384Sha384, &p256.public_pem).is_err());
    assert!(validate_sign_public_key(AlgorithmId::Sm2Sign, &p256.public_pem).is_err());
    assert!(validate_sign_public_key(AlgorithmId::EcdsaP256Sha256, &sm2.public_pem).is_err());
    assert!(
        validate_sign_public_key(AlgorithmId::RsaPkcs1v15SignSha256, &p256.public_pem).is_err()
    );
    assert!(matches!(
        validate_sign_public_key(AlgorithmId::HmacSha256, &p256.public_pem),
        Err(CryptoError::Unsupported { .. })
    ));
    assert!(matches!(
        generate_sign_keypair(AlgorithmId::Aes256Gcm),
        Err(CryptoError::Unsupported { .. })
    ));
    assert!(validate_sign_public_key(AlgorithmId::Ed25519, "not base64 !!").is_err());
    assert!(validate_sign_public_key(AlgorithmId::Ed25519, "   ").is_err());
}

#[test]
fn rsa_public_key_below_minimum_bits_is_rejected() {
    use rsa::pkcs8::{EncodePublicKey, LineEnding};
    let small = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).expect("rsa 1024");
    let pem = rsa::RsaPublicKey::from(&small)
        .to_public_key_pem(LineEnding::LF)
        .expect("pem");
    let err = validate_sign_public_key(AlgorithmId::RsaPkcs1v15SignSha256, &pem).unwrap_err();
    assert!(err.to_string().contains("2048"), "{err}");
}

#[test]
fn sm2_raw_and_pem_encodings_are_interchangeable() {
    let ctx = libsm::sm2::signature::SigCtx::new();
    let (pk, sk) = ctx.new_keypair().expect("sm2 keypair");
    let scalar = ctx.serialize_seckey(&sk).expect("sk");
    let point = ctx.serialize_pubkey(&pk, false).expect("pk");
    let private_pem = sm2_private_key_to_pem(&scalar).expect("private pem");
    let public_pem = sm2_public_key_to_pem(&point).expect("public pem");
    let signer = asym_signer(AlgorithmId::Sm2Sign).expect("signer");
    let msg = b"sm2 raw/pem";
    let sig_pem = signer.sign(&text_key(&private_pem), msg).expect("sign pem");
    let sig_raw = signer
        .sign(&text_key(&STANDARD.encode(&scalar)), msg)
        .expect("sign raw");
    for sig in [&sig_pem, &sig_raw] {
        signer
            .verify(&text_key(&public_pem), msg, sig)
            .expect("verify pem");
        signer
            .verify(&text_key(&STANDARD.encode(&point)), msg, sig)
            .expect("verify raw");
    }
    validate_sign_public_key(AlgorithmId::Sm2Sign, &STANDARD.encode(&point)).expect("raw pub");
}

#[test]
fn openssl_generated_signatures_verify() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/interop/openssl_sign_interop.json"
    ))
    .expect("fixture");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let msg = STANDARD
        .decode(fixture["message_b64"].as_str().expect("msg"))
        .expect("msg b64");
    let cases = fixture["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 4);
    for case in cases {
        let alg: AlgorithmId = case["alg"].as_str().expect("alg").parse().expect("alg id");
        let public = case["public_pem"].as_str().expect("pem");
        let sig = STANDARD
            .decode(case["signature_b64"].as_str().expect("sig"))
            .expect("sig b64");
        validate_sign_public_key(alg, public).expect("openssl public valid");
        let signer = asym_signer(alg).expect("signer");
        signer
            .verify(&text_key(public), &msg, &sig)
            .unwrap_or_else(|e| panic!("{alg} openssl interop: {e}"));
        assert!(signer.verify(&text_key(public), b"x", &sig).is_err());
    }
}
