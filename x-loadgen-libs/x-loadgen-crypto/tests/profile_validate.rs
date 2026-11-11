//! 租户传输 Profile 校验与命名套件解析。

use x_loadgen_crypto::{
    AlgorithmId, AlgorithmRegistry, BindingSpec, CryptoError, KeySelection, SUITE_GM_SM4_HMAC_SM3,
    SUITE_TENANT_CHACHA20, SUITE_TENANT_DEFAULT_2026_11, TenantTransportProfile, TransportMode,
    known_suites, resolve_suite, suite_data_alg, suite_mac_alg,
};

fn registry() -> AlgorithmRegistry {
    AlgorithmRegistry::default_registry()
}

#[test]
fn resolve_known_suites_and_reject_unknown() {
    let def = resolve_suite(SUITE_TENANT_DEFAULT_2026_11).expect("default");
    assert_eq!(def.data_alg, AlgorithmId::Aes256Gcm);
    assert_eq!(def.mac_alg, None);

    let gm = resolve_suite(SUITE_GM_SM4_HMAC_SM3).expect("gm");
    assert_eq!(gm.data_alg, AlgorithmId::Sm4CbcPkcs7);
    assert_eq!(gm.mac_alg, Some(AlgorithmId::HmacSm3));

    let chacha = resolve_suite(SUITE_TENANT_CHACHA20).expect("chacha");
    assert_eq!(chacha.data_alg, AlgorithmId::Chacha20Poly1305);
    assert_eq!(chacha.mac_alg, None);

    assert_eq!(
        suite_data_alg(SUITE_TENANT_DEFAULT_2026_11).expect("data"),
        AlgorithmId::Aes256Gcm
    );
    assert_eq!(
        suite_mac_alg(SUITE_GM_SM4_HMAC_SM3).expect("mac"),
        Some(AlgorithmId::HmacSm3)
    );
    assert_eq!(suite_mac_alg(SUITE_TENANT_CHACHA20).expect("no mac"), None);

    let err = resolve_suite("suite/unknown").expect_err("unknown");
    assert!(matches!(
        err,
        CryptoError::UnknownSuite { name } if name == "suite/unknown"
    ));
    let err = suite_data_alg("suite/unknown").expect_err("unknown data");
    assert!(matches!(err, CryptoError::UnknownSuite { .. }));

    assert_eq!(
        known_suites(),
        &[
            SUITE_TENANT_DEFAULT_2026_11,
            SUITE_GM_SM4_HMAC_SM3,
            SUITE_TENANT_CHACHA20
        ]
    );
}

#[test]
fn tenant_plaintext_and_default_suite_ok() {
    let reg = registry();
    TenantTransportProfile::plaintext()
        .validate(&reg)
        .expect("plaintext");

    let mut profile =
        TenantTransportProfile::from_named_suite(SUITE_TENANT_DEFAULT_2026_11, TransportMode::Full)
            .expect("suite");
    profile.key = Some(KeySelection::Latest {
        key_id: "tenant-main".to_owned(),
    });
    profile.validate(&reg).expect("aes-gcm full");

    let mut gm =
        TenantTransportProfile::from_named_suite(SUITE_GM_SM4_HMAC_SM3, TransportMode::Request)
            .expect("gm suite");
    gm.key = Some(KeySelection::Fixed {
        key_id: "gm-key".to_owned(),
        key_version: 1,
    });
    gm.validate(&reg).expect("gm suite");
}

#[test]
fn tenant_rejects_cbc_without_mac_and_legacy_without_allow() {
    let reg = registry();

    let mut cbc = TenantTransportProfile::plaintext();
    cbc.mode = TransportMode::Request;
    cbc.data_alg = Some(AlgorithmId::Aes256Cbc);
    cbc.key = Some(KeySelection::Latest {
        key_id: "k".to_owned(),
    });
    let err = cbc.validate(&reg).expect_err("cbc needs mac");
    assert!(
        matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("requires mac_alg"))
    );

    cbc.mac_alg = Some(AlgorithmId::HmacSha256);
    cbc.validate(&reg).expect("cbc+hmac");

    let mut legacy = cbc.clone();
    legacy.data_alg = Some(AlgorithmId::Aes128Ecb);
    legacy.mac_alg = Some(AlgorithmId::HmacSha256);
    legacy.allow_insecure = false;
    let err = legacy.validate(&reg).expect_err("legacy");
    assert!(matches!(
        err,
        CryptoError::LegacyNotAllowed {
            id: AlgorithmId::Aes128Ecb
        }
    ));
    legacy.allow_insecure = true;
    legacy.validate(&reg).expect("legacy allowed");
}

#[test]
fn tenant_rejects_mode_none_with_signature_or_algs() {
    let reg = registry();

    let mut p = TenantTransportProfile::plaintext();
    p.require_signature = true;
    let err = p.validate(&reg).expect_err("sig");
    assert!(
        matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("require_signature"))
    );

    let mut p = TenantTransportProfile::plaintext();
    p.data_alg = Some(AlgorithmId::Aes256Gcm);
    let err = p.validate(&reg).expect_err("alg");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("mode=none")));

    let err =
        TenantTransportProfile::from_named_suite(SUITE_TENANT_DEFAULT_2026_11, TransportMode::None)
            .expect_err("none+suite ctor");
    assert!(matches!(err, CryptoError::InvalidProfile { .. }));
}

#[test]
fn tenant_rejects_suite_alg_conflict_and_missing_key() {
    let reg = registry();
    let mut p = TenantTransportProfile::from_named_suite(
        SUITE_TENANT_DEFAULT_2026_11,
        TransportMode::Request,
    )
    .expect("suite");
    p.data_alg = Some(AlgorithmId::Chacha20Poly1305);
    let err = p.validate(&reg).expect_err("conflict");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("conflicts")));

    let mut p =
        TenantTransportProfile::from_named_suite(SUITE_TENANT_CHACHA20, TransportMode::Request)
            .expect("chacha");
    let err = p.validate(&reg).expect_err("no key");
    assert!(
        matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("key selection"))
    );

    p.key = Some(KeySelection::Latest {
        key_id: String::new(),
    });
    let err = p.validate(&reg).expect_err("empty key");
    assert!(matches!(err, CryptoError::InvalidProfile { reason } if reason.contains("key_id")));

    p.key = Some(KeySelection::Latest {
        key_id: "ok".to_owned(),
    });
    p.binding = Some(BindingSpec { required: true });
    p.validate(&reg).expect("chacha ok");
}
