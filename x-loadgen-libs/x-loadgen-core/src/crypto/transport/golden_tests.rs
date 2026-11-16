//! mazbri 金样向量：`golden_mazbri.json` 由 mazbri 真实实现（`mazbri-core` 的
//! `seal_envelope`）对固定密钥 / 租户 / 路径 / 明文生成，原样落盘。
//!
//! 断言两件事，任何一侧单方面改动信封协议（AAD、签名规范串、密钥截取、编码）都会失败：
//! 1. x-loadgen `open_envelope` 能解开 mazbri 产出的信封（网关 → x-loadgen 方向）；
//! 2. x-loadgen 以相同 IV / nonce 密封，逐字节等于 mazbri 的信封（x-loadgen → 网关方向）。
//!
//! mockgw 与 x-loadgen 共用本模块，自测自无法发现与真实网关的漂移，故以此兜底。

use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    tenant_id: String,
    http_path: String,
    key_version: String,
    timestamp: i64,
    secret_hex: String,
    plaintext: String,
    cases: Vec<GoldenCase>,
}

#[derive(Deserialize)]
struct GoldenCase {
    name: String,
    profile: TenantTransportProfile,
    envelope: TransportEnvelope,
}

fn golden() -> Golden {
    serde_json::from_str(include_str!("golden_mazbri.json")).expect("golden json")
}

fn secret(golden: &Golden) -> Vec<u8> {
    hex::decode(&golden.secret_hex).expect("secret hex")
}

#[test]
fn golden_covers_aead_and_mac_algorithms() {
    let golden = golden();
    let algs: Vec<&str> = golden
        .cases
        .iter()
        .map(|case| case.envelope.data_alg.as_str())
        .collect();
    for alg in [
        "aes-256-gcm",
        "aes-192-gcm",
        "aes-128-gcm",
        "chacha20-poly1305",
        "sm4-cbc-pkcs7",
        "aes-256-cbc",
    ] {
        assert!(algs.contains(&alg), "golden vectors miss {alg}");
    }
    assert!(
        golden
            .cases
            .iter()
            .any(|case| case.envelope.mac_alg.is_some()
                && parse_algorithm_id(&case.envelope.data_alg)
                    .is_ok_and(|id| id.kind() == AlgorithmKind::Aead)),
        "golden vectors miss AEAD + MAC"
    );
}

#[test]
fn opens_envelopes_sealed_by_mazbri() {
    let golden = golden();
    let registry = AlgorithmRegistry::default_registry();
    let secret = secret(&golden);
    for case in &golden.cases {
        let opened = open_envelope(
            &case.profile,
            &registry,
            &secret,
            Some(&secret),
            &golden.tenant_id,
            &golden.http_path,
            &case.envelope,
        )
        .unwrap_or_else(|err| panic!("{}: open mazbri envelope: {err}", case.name));
        assert_eq!(opened, golden.plaintext.as_bytes(), "{}", case.name);

        assert!(
            open_envelope(
                &case.profile,
                &registry,
                &secret,
                Some(&secret),
                "another-tenant",
                &golden.http_path,
                &case.envelope,
            )
            .is_err(),
            "{}: tenant_id must be authenticated",
            case.name
        );
    }
}

#[test]
fn seals_byte_identical_to_mazbri() {
    let golden = golden();
    let registry = AlgorithmRegistry::default_registry();
    let secret = secret(&golden);
    for case in &golden.cases {
        let iv = STANDARD.decode(&case.envelope.iv).expect("golden iv");
        let sealed = seal_envelope_with(
            SealEnvelopeParams {
                profile: &case.profile,
                registry: &registry,
                data_key: &secret,
                mac_key: Some(&secret),
                key_version: &golden.key_version,
                tenant_id: &golden.tenant_id,
                http_path: &golden.http_path,
                plaintext: golden.plaintext.as_bytes(),
                timestamp: golden.timestamp,
            },
            |len| {
                assert_eq!(len, iv.len(), "{}: iv length", case.name);
                iv.clone()
            },
            case.envelope.nonce.clone(),
        )
        .unwrap_or_else(|err| panic!("{}: seal: {err}", case.name));
        assert_eq!(
            serde_json::to_value(&sealed).expect("sealed json"),
            serde_json::to_value(&case.envelope).expect("golden json"),
            "{}: envelope differs from mazbri",
            case.name
        );
    }
}
