//! AES-CBC / CTR / CFB 向量（NIST SP 800-38A）与 `require_mac_for`。

use x_loadgen_crypto::{
    AlgorithmId, AlgorithmRegistry, BlockCipher, CfbParams, KeyPurpose, KeyRef, SymmetricAlgorithm,
    decrypt_cfb, encrypt_cfb, require_mac_for,
};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.replace([' ', '\n', '_'], "")).expect("hex")
}

fn data_key(bytes: &[u8]) -> KeyRef {
    KeyRef::raw(KeyPurpose::DataEncryption, bytes.to_vec())
}

#[test]
fn require_mac_for_cbc_ctr_cfb() {
    for id in [
        AlgorithmId::Aes128Cbc,
        AlgorithmId::Aes256Cbc,
        AlgorithmId::Aes128Ctr,
        AlgorithmId::Aes256Ctr,
        AlgorithmId::Aes128Cfb,
        AlgorithmId::Aes256Cfb,
    ] {
        assert!(require_mac_for(id), "{id} must require MAC");
        let registry = AlgorithmRegistry::default_registry();
        let meta = registry.get(id).expect("registered");
        assert!(meta.require_mac);
    }
    assert!(!require_mac_for(AlgorithmId::Aes256Gcm));
}

#[test]
fn aes128_cbc_nist_sp800_38a() {
    let key = unhex("2b7e151628aed2a6abf7158809cf4f3c");
    let iv = unhex("000102030405060708090a0b0c0d0e0f");
    let pt = unhex(
        "6bc1bee22e409f96e93d7e117393172a\
         ae2d8a571e03ac9c9eb76fac45af8e51\
         30c81c46a35ce411e5fbc1191a0a52ef\
         f69f2445df4f9b17ad2b417be66c3710",
    );
    // NIST F.2.1 ciphertext (64) + PKCS7 padding block
    let expected = unhex(
        "7649abac8119b246cee98e9b12e9197d\
         5086cb9b507219ee95db113a917678b2\
         73bed6b8e3c1743b7116e69e22229516\
         3ff1caa1681fac09120eca307586e1a7\
         8cb82807230e1321d3fae00d18cc2012",
    );
    let registry = AlgorithmRegistry::default_registry();
    let cipher = registry
        .block_cipher(AlgorithmId::Aes128Cbc)
        .expect("cipher");
    let ct = cipher.encrypt(&data_key(&key), &iv, &pt).expect("enc");
    assert_eq!(ct, expected);
    let opened = cipher.decrypt(&data_key(&key), &iv, &ct).expect("dec");
    assert_eq!(opened, pt);
}

#[test]
fn aes128_ctr_nist_sp800_38a() {
    let key = unhex("2b7e151628aed2a6abf7158809cf4f3c");
    let iv = unhex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    let pt = unhex(
        "6bc1bee22e409f96e93d7e117393172a\
         ae2d8a571e03ac9c9eb76fac45af8e51\
         30c81c46a35ce411e5fbc1191a0a52ef\
         f69f2445df4f9b17ad2b417be66c3710",
    );
    let expected = unhex(
        "874d6191b620e3261bef6864990db6ce\
         9806f66b7970fdff8617187bb9fffdff\
         5ae4df3edbd5d35e5b4f09020db03eab\
         1e031dda2fbe03d1792170a0f3009cee",
    );
    let cipher = SymmetricAlgorithm::Aes128Ctr;
    let ct = cipher.encrypt(&data_key(&key), &iv, &pt).expect("enc");
    assert_eq!(ct, expected);
    let opened = cipher.decrypt(&data_key(&key), &iv, &ct).expect("dec");
    assert_eq!(opened, pt);
}

#[test]
fn aes128_cfb128_nist_and_cfb8_params() {
    let key = unhex("2b7e151628aed2a6abf7158809cf4f3c");
    let iv = unhex("000102030405060708090a0b0c0d0e0f");
    let pt = unhex(
        "6bc1bee22e409f96e93d7e117393172a\
         ae2d8a571e03ac9c9eb76fac45af8e51\
         30c81c46a35ce411e5fbc1191a0a52ef\
         f69f2445df4f9b17ad2b417be66c3710",
    );
    let expected128 = unhex(
        "3b3fd92eb72dad20333449f8e83cfb4a\
         c8a64537a0b3a93fcde3cdad9f1ce58b\
         26751f67a3cbb140b1808cf187a4f4df\
         c04b05357c5d1c0eeac4c66f9ff7f2e6",
    );
    let cipher = SymmetricAlgorithm::Aes128Cfb;
    let ct = cipher.encrypt(&data_key(&key), &iv, &pt).expect("enc");
    assert_eq!(ct, expected128);

    let pt16 = &pt[..16];
    let expected8 = unhex("3b79424c9c0dd436bace9e0ed4586a4f");
    let ct8 = encrypt_cfb(
        SymmetricAlgorithm::Aes128Cfb,
        &data_key(&key),
        &iv,
        pt16,
        CfbParams::eight_bit(),
    )
    .expect("cfb8");
    assert_eq!(ct8, expected8);
    let opened = decrypt_cfb(
        SymmetricAlgorithm::Aes128Cfb,
        &data_key(&key),
        &iv,
        &ct8,
        CfbParams::eight_bit(),
    )
    .expect("dec");
    assert_eq!(opened, pt16);
}

#[test]
fn aes256_modes_roundtrip() {
    let key = unhex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4");
    let iv = unhex("000102030405060708090a0b0c0d0e0f");
    let ctr_iv = unhex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    let pt = unhex("6bc1bee22e409f96e93d7e117393172a");
    let registry = AlgorithmRegistry::default_registry();

    let cbc = registry.block_cipher(AlgorithmId::Aes256Cbc).expect("cbc");
    let ct = cbc.encrypt(&data_key(&key), &iv, &pt).expect("enc");
    assert_eq!(
        ct,
        unhex("f58c4c04d6e5f1ba779eabfb5f7bfbd6485a5c81519cf378fa36d42b8547edc0")
    );
    assert_eq!(cbc.decrypt(&data_key(&key), &iv, &ct).expect("dec"), pt);

    let ctr = registry.block_cipher(AlgorithmId::Aes256Ctr).expect("ctr");
    let ct = ctr.encrypt(&data_key(&key), &ctr_iv, &pt).expect("enc");
    assert_eq!(ct, unhex("601ec313775789a5b7a7f504bbf3d228"));
    assert_eq!(ctr.decrypt(&data_key(&key), &ctr_iv, &ct).expect("dec"), pt);

    let cfb = registry.block_cipher(AlgorithmId::Aes256Cfb).expect("cfb");
    let ct = cfb.encrypt(&data_key(&key), &iv, &pt).expect("enc");
    assert_eq!(ct, unhex("dc7e84bfda79164b7ecd8486985d3860"));
    assert_eq!(cfb.decrypt(&data_key(&key), &iv, &ct).expect("dec"), pt);
}
