//! mazbri 金样向量：`golden_mazbri.json` 由 mazbri 规范（`mazbri-core` 的
//! `seal_envelope` 四向 HKDF 派生与全元数据绑定）对固定密钥 / 租户 / 路径 / 明文生成，原样落盘。
//!
//! 断言三件事，任何一侧单方面改动信封协议（HKDF 子密钥、AAD、签名规范串、方向、编码）都会失败：
//! 1. x-loadgen `open_envelope` 能解开 mazbri 产出的信封；
//! 2. 换方向（反射重放）或换 `tenant_id` 解封均失败；
//! 3. x-loadgen 以相同 IV / nonce 密封，逐字节等于金样信封。

use super::*;
use serde::Deserialize;

#[derive(Serialize, Deserialize)]
struct Golden {
    source: String,
    tenant_id: String,
    http_path: String,
    key_version: String,
    timestamp: i64,
    secret_hex: String,
    plaintext: String,
    cases: Vec<GoldenCase>,
}

const fn default_direction() -> EnvelopeDirection {
    EnvelopeDirection::ClientToGateway
}

const fn opposite_direction(dir: EnvelopeDirection) -> EnvelopeDirection {
    match dir {
        EnvelopeDirection::ClientToGateway => EnvelopeDirection::GatewayToClient,
        EnvelopeDirection::GatewayToClient => EnvelopeDirection::ClientToGateway,
    }
}

#[derive(Serialize, Deserialize)]
struct GoldenCase {
    name: String,
    #[serde(default = "default_direction")]
    direction: EnvelopeDirection,
    profile: TenantTransportProfile,
    envelope: TransportEnvelope,
}

fn golden_file_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/crypto/transport/golden_mazbri.json")
}

fn golden() -> Golden {
    let path = golden_file_path();
    let raw = std::fs::read_to_string(&path).expect("read golden_mazbri.json");
    let mut doc: Golden = serde_json::from_str(&raw).expect("golden json");
    if std::env::var("XLG_REGEN_GOLDEN").as_deref() == Ok("1") {
        let registry = AlgorithmRegistry::default_registry();
        let secret_bytes = hex::decode(&doc.secret_hex).expect("secret hex");
        let base_cases: Vec<GoldenCase> = doc
            .cases
            .into_iter()
            .filter(|c| !c.name.ends_with("-g2c"))
            .collect();
        let mut rebuilt = Vec::with_capacity(base_cases.len() + 1);
        for mut c in base_cases {
            let iv = STANDARD.decode(&c.envelope.iv).expect("iv");
            c.direction = EnvelopeDirection::ClientToGateway;
            c.envelope = seal_envelope_with(
                SealEnvelopeParams {
                    profile: &c.profile,
                    registry: &registry,
                    master_key: &secret_bytes,
                    direction: c.direction,
                    key_version: &doc.key_version,
                    tenant_id: &doc.tenant_id,
                    http_path: &doc.http_path,
                    plaintext: doc.plaintext.as_bytes(),
                    timestamp: doc.timestamp,
                },
                |_| iv,
                c.envelope.nonce.clone(),
            )
            .expect("seal c2g");
            rebuilt.push(c);
        }
        let first = &rebuilt[0];
        let iv0 = STANDARD.decode(&first.envelope.iv).expect("iv0");
        let g2c_env = seal_envelope_with(
            SealEnvelopeParams {
                profile: &first.profile,
                registry: &registry,
                master_key: &secret_bytes,
                direction: EnvelopeDirection::GatewayToClient,
                key_version: &doc.key_version,
                tenant_id: &doc.tenant_id,
                http_path: &doc.http_path,
                plaintext: doc.plaintext.as_bytes(),
                timestamp: doc.timestamp,
            },
            |_| iv0,
            first.envelope.nonce.clone(),
        )
        .expect("seal g2c");
        rebuilt.push(GoldenCase {
            name: format!("{}-g2c", first.name),
            direction: EnvelopeDirection::GatewayToClient,
            profile: first.profile.clone(),
            envelope: g2c_env,
        });
        doc.source =
            "mazbri TransportEnvelope v1 (4-way HKDF c2g/g2c + full metadata AAD/MAC binding)"
                .to_owned();
        doc.cases = rebuilt;
        let mut text = serde_json::to_string_pretty(&doc).expect("serialize golden");
        text.push('\n');
        std::fs::write(&path, text).expect("write golden_mazbri.json");
    }
    doc
}

fn secret(golden: &Golden) -> Vec<u8> {
    hex::decode(&golden.secret_hex).expect("secret hex")
}

#[test]
fn golden_covers_aead_mac_and_both_directions() {
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
    assert!(
        golden
            .cases
            .iter()
            .any(|c| c.direction == EnvelopeDirection::ClientToGateway)
            && golden
                .cases
                .iter()
                .any(|c| c.direction == EnvelopeDirection::GatewayToClient),
        "golden vectors must cover both c2g and g2c directions"
    );
}

#[test]
fn opens_envelopes_sealed_by_mazbri_and_rejects_reflection() {
    let golden = golden();
    let registry = AlgorithmRegistry::default_registry();
    let secret = secret(&golden);
    for case in &golden.cases {
        let opened = open_envelope(
            &case.profile,
            &registry,
            &secret,
            case.direction,
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
                opposite_direction(case.direction),
                &golden.tenant_id,
                &golden.http_path,
                &case.envelope,
            )
            .is_err(),
            "{}: direction reflection must be rejected",
            case.name
        );

        assert!(
            open_envelope(
                &case.profile,
                &registry,
                &secret,
                case.direction,
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
                master_key: &secret,
                direction: case.direction,
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
