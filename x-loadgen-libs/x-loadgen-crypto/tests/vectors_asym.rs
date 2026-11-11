//! RSA `PKCS#1 v1.5` 签名、ECDSA P-256/P-384、Ed25519 往返向量。

use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::rand_core::{OsRng, RngCore};
use x_loadgen_crypto::{AsymSign, AsymSignAlgorithm, CryptoError, KeyPurpose, KeyRef};

fn rsa_keypair_pem() -> (String, String) {
    let mut rng = OsRng;
    let private = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
    let public = rsa::RsaPublicKey::from(&private);
    let sk = private
        .to_pkcs8_pem(LineEnding::LF)
        .expect("sk pem")
        .to_string();
    let pk = public.to_public_key_pem(LineEnding::LF).expect("pk pem");
    (sk, pk)
}

fn fill_rng(buf: &mut [u8]) {
    OsRng.fill_bytes(buf);
}

#[test]
fn rsa_pkcs1v15_sign_sha_variants() {
    let (sk_pem, pk_pem) = rsa_keypair_pem();
    let signer = KeyRef::pem(KeyPurpose::AsymSign, sk_pem);
    let verifier = KeyRef::pem(KeyPurpose::AsymSign, pk_pem);
    let msg = b"rsa-sign-message";

    for alg in [
        AsymSignAlgorithm::RsaPkcs1v15SignSha256,
        AsymSignAlgorithm::RsaPkcs1v15SignSha384,
        AsymSignAlgorithm::RsaPkcs1v15SignSha512,
    ] {
        let sig = alg.sign(&signer, msg).expect("sign");
        alg.verify(&verifier, msg, &sig).expect("verify");
        let err = alg.verify(&verifier, b"nope", &sig).expect_err("tamper");
        assert!(matches!(err, CryptoError::AuthenticationFailed));
    }
}

#[test]
fn ecdsa_p256_raw_roundtrip() {
    use p256::ecdsa::SigningKey;
    let mut scalar = [0u8; 32];
    loop {
        fill_rng(&mut scalar);
        if scalar.iter().any(|b| *b != 0)
            && let Ok(sk) = SigningKey::from_bytes((&scalar).into())
        {
            let vk = sk.verifying_key();
            let pk_bytes = vk.to_encoded_point(false).as_bytes().to_vec();
            let alg = AsymSignAlgorithm::EcdsaP256Sha256;
            let msg = b"p256-msg";
            let sig = alg
                .sign(&KeyRef::raw(KeyPurpose::AsymSign, scalar.to_vec()), msg)
                .expect("sign");
            alg.verify(&KeyRef::raw(KeyPurpose::AsymSign, pk_bytes), msg, &sig)
                .expect("verify");
            break;
        }
    }
}

#[test]
fn ecdsa_p384_raw_roundtrip() {
    use p384::ecdsa::SigningKey;
    let mut scalar = [0u8; 48];
    loop {
        fill_rng(&mut scalar);
        if scalar.iter().any(|b| *b != 0)
            && let Ok(sk) = SigningKey::from_bytes((&scalar).into())
        {
            let vk = sk.verifying_key();
            let pk_bytes = vk.to_encoded_point(false).as_bytes().to_vec();
            let alg = AsymSignAlgorithm::EcdsaP384Sha384;
            let msg = b"p384-msg";
            let sig = alg
                .sign(&KeyRef::raw(KeyPurpose::AsymSign, scalar.to_vec()), msg)
                .expect("sign");
            alg.verify(&KeyRef::raw(KeyPurpose::AsymSign, pk_bytes), msg, &sig)
                .expect("verify");
            break;
        }
    }
}

#[test]
fn ed25519_raw_roundtrip() {
    use ed25519_dalek::{SigningKey, VerifyingKey};
    let mut seed = [0u8; 32];
    fill_rng(&mut seed);
    let sk = SigningKey::from_bytes(&seed);
    let vk: VerifyingKey = sk.verifying_key();
    let alg = AsymSignAlgorithm::Ed25519;
    let msg = b"ed25519-msg";
    let sig = alg
        .sign(
            &KeyRef::raw(KeyPurpose::AsymSign, sk.to_bytes().to_vec()),
            msg,
        )
        .expect("sign");
    alg.verify(
        &KeyRef::raw(KeyPurpose::AsymSign, vk.to_bytes().to_vec()),
        msg,
        &sig,
    )
    .expect("verify");
}
