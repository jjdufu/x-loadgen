//! Gateway-side cases for the upupt test upstream.
//!
//! The bind file maps `face.method` to a stable upstream channel code. Gateway
//! product ids change on every rebuild, so they come from a product map that is
//! generated at runtime from mgmt (channel code -> product id). This module does
//! not speak any upstream protocol and does not embed a tenant.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::json;

use crate::config::ProductConfig;
use crate::expect::FieldCheck;

const FACES: &[&str] = &[
    "rest", "oauth", "jsonrpc", "graphql", "job", "sign", "soap", "ws", "grpc",
];

const METHODS: &[&str] = &["posts", "photos", "imgdef"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    /// Nine faces, three methods, positive bodies only.
    Pressure,
    /// Pressure plus imgdef mismatch and invalid posts on every face.
    Coverage,
    /// REST positive cases, for ratelimit / cache runs.
    Rest,
}

impl Suite {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "pressure" => Some(Self::Pressure),
            "coverage" => Some(Self::Coverage),
            "rest" => Some(Self::Rest),
            _ => None,
        }
    }

    fn faces(self) -> &'static [&'static str] {
        match self {
            Self::Rest => &["rest"],
            Self::Pressure | Self::Coverage => FACES,
        }
    }

    fn include_negative(self) -> bool {
        matches!(self, Self::Coverage)
    }
}

#[derive(Debug, Deserialize)]
struct BindFile {
    /// Dotted keys such as `rest.posts` are nested tables in TOML.
    /// Flatten them back to `face.method` after parse.
    bind: toml::Value,
    #[serde(default)]
    images: ImageDirs,
    /// Faces to run. Unset means every face of the suite.
    #[serde(default)]
    faces: Option<Vec<String>>,
    /// Per-method QPS cap written into each product's `qps_limit`.
    #[serde(default)]
    qps: MethodQps,
}

/// Runtime-generated `[products]` table keyed by channel code.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductMap {
    products: BTreeMap<String, Target>,
}

/// Gateway product id and request path for one channel code.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    product_id: String,
    path: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MethodQps {
    posts: Option<u32>,
    photos: Option<u32>,
    imgdef: Option<u32>,
}

impl MethodQps {
    fn get(&self, method: &str) -> Option<u32> {
        match method {
            "posts" => self.posts,
            "photos" => self.photos,
            "imgdef" => self.imgdef,
            _ => None,
        }
    }

    fn validate(&self) -> Result<()> {
        for method in METHODS {
            if self.get(method) == Some(0) {
                bail!("qps.{method} must be greater than 0 when set");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ImageDirs {
    #[serde(default = "default_match_dir")]
    match_dir: String,
    #[serde(default = "default_mismatch_dir")]
    mismatch_dir: String,
}

impl Default for ImageDirs {
    fn default() -> Self {
        Self {
            match_dir: default_match_dir(),
            mismatch_dir: default_mismatch_dir(),
        }
    }
}

fn default_match_dir() -> String {
    "../imgs".into()
}

fn default_mismatch_dir() -> String {
    "../imgs/nomatch".into()
}

pub fn load_cases(
    preset: &str,
    suite_raw: &str,
    bind_path: &Path,
    map_path: &Path,
) -> Result<Vec<ProductConfig>> {
    if !preset.eq_ignore_ascii_case("upupt") {
        bail!("unknown preset '{preset}', known: upupt");
    }
    let suite = Suite::parse(suite_raw)
        .ok_or_else(|| anyhow!("unknown suite '{suite_raw}', known: pressure, coverage, rest"))?;
    let raw = fs::read_to_string(bind_path)
        .with_context(|| format!("failed to read bind file {}", bind_path.display()))?;
    let bind: BindFile = toml::from_str(&raw)
        .with_context(|| format!("failed to parse bind file {}", bind_path.display()))?;
    let flat = flatten_bind(&bind.bind)?;
    let raw_map = fs::read_to_string(map_path)
        .with_context(|| format!("failed to read product map {}", map_path.display()))?;
    let map: ProductMap = toml::from_str(&raw_map)
        .with_context(|| format!("failed to parse product map {}", map_path.display()))?;
    let resolved = resolve_products(&flat, &map.products)?;
    expand(suite, &resolved, &bind)
}

/// Replace every bind value (a channel code) with its product id. Every
/// unmapped code is reported at once so a single regeneration fixes them all.
fn resolve_products(
    bind: &BTreeMap<String, String>,
    products: &BTreeMap<String, Target>,
) -> Result<BTreeMap<String, Target>> {
    let known = known_keys();
    for key in bind.keys() {
        if !known.iter().any(|item| item == key) {
            bail!("unknown bind key '{key}'");
        }
    }
    let mut resolved = BTreeMap::new();
    let mut missing = Vec::new();
    for (key, code) in bind {
        let code = code.trim();
        match products.get(code) {
            Some(target)
                if !target.product_id.trim().is_empty() && target.path.starts_with('/') =>
            {
                resolved.insert(
                    key.clone(),
                    Target {
                        product_id: target.product_id.trim().to_string(),
                        path: target.path.clone(),
                    },
                );
            }
            _ => missing.push(format!("{key}={code}")),
        }
    }
    if !missing.is_empty() {
        bail!(
            "product map has no product for channel code(s): {}",
            missing.join(", ")
        );
    }
    Ok(resolved)
}

fn flatten_bind(value: &toml::Value) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    flatten_bind_into("", value, &mut out)?;
    Ok(out)
}

fn flatten_bind_into(
    prefix: &str,
    value: &toml::Value,
    out: &mut BTreeMap<String, String>,
) -> Result<()> {
    match value {
        toml::Value::String(text) => {
            if prefix.is_empty() {
                bail!("bind entry is missing a key");
            }
            out.insert(prefix.to_string(), text.clone());
            Ok(())
        }
        toml::Value::Table(table) => {
            for (key, child) in table {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_bind_into(&next, child, out)?;
            }
            Ok(())
        }
        other => bail!("bind value at '{prefix}' must be a string, got {other}"),
    }
}

fn expand(
    suite: Suite,
    bind: &BTreeMap<String, Target>,
    file: &BindFile,
) -> Result<Vec<ProductConfig>> {
    file.qps.validate()?;
    let faces = select_faces(suite, file.faces.as_deref())?;
    let images = &file.images;
    let qps = &file.qps;
    let mut products = Vec::new();
    for face in faces {
        for method in METHODS {
            let target = require_target(bind, &key(face, method))?;
            let mut product = positive(face, method, target, images);
            product.qps_limit = qps.get(method);
            products.push(product);
        }
        if suite.include_negative() {
            let posts = require_target(bind, &key(face, "posts"))?;
            let imgdef = require_target(bind, &key(face, "imgdef"))?;
            let mut invalid = invalid_posts(face, posts);
            invalid.qps_limit = qps.get("posts");
            products.push(invalid);
            let mut mismatch = mismatch_imgdef(face, imgdef, images);
            mismatch.qps_limit = qps.get("imgdef");
            products.push(mismatch);
        }
    }
    Ok(products)
}

/// Suite faces filtered by the bind file `faces` list, in suite order.
fn select_faces(suite: Suite, wanted: Option<&[String]>) -> Result<Vec<&'static str>> {
    let Some(wanted) = wanted else {
        return Ok(suite.faces().to_vec());
    };
    if wanted.is_empty() {
        bail!("faces must not be empty when set");
    }
    for face in wanted {
        if !FACES.contains(&face.as_str()) {
            bail!(
                "unknown face '{face}' in faces, known: {}",
                FACES.join(", ")
            );
        }
    }
    let selected: Vec<&'static str> = suite
        .faces()
        .iter()
        .copied()
        .filter(|face| wanted.iter().any(|item| item == face))
        .collect();
    if selected.is_empty() {
        bail!("faces selects nothing for this suite");
    }
    Ok(selected)
}

fn known_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for face in FACES {
        for method in METHODS {
            keys.push(key(face, method));
        }
    }
    keys
}

fn key(face: &str, method: &str) -> String {
    format!("{face}.{method}")
}

fn require_target(bind: &BTreeMap<String, Target>, key: &str) -> Result<Target> {
    bind.get(key)
        .cloned()
        .ok_or_else(|| anyhow!("bind file missing {key}"))
}

fn positive(face: &str, method: &str, target: Target, images: &ImageDirs) -> ProductConfig {
    let mut product = base(target, &format!("upupt {face} {method}"));
    match method {
        "posts" => {
            product.body_json = Some(posts_body(face, false));
            product.expect_checks = posts_checks(face);
            product.timeout_secs = Some(timeout_secs(face, method));
        }
        "photos" => {
            product.body_json = Some(photos_body());
            product.expect_checks = photos_checks(face);
            product.timeout_secs = Some(timeout_secs(face, method));
        }
        "imgdef" => {
            product.body_json = Some(imgdef_body());
            product.body_image_dir = Some(images.match_dir.clone());
            product.body_image_field = Some("image".into());
            product.body_image_name_field = Some("filename".into());
            product.expect_checks = imgdef_checks(face, true);
            product.timeout_secs = Some(timeout_secs(face, method));
        }
        _ => unreachable!("method list is closed"),
    }
    product
}

fn invalid_posts(face: &str, target: Target) -> ProductConfig {
    let mut product = base(target, &format!("upupt {face} posts invalid"));
    product.body_json = Some(posts_body(face, true));
    product.expect_transport = "reject".into();
    product.timeout_secs = Some(timeout_secs(face, "posts"));
    product
}

fn mismatch_imgdef(face: &str, target: Target, images: &ImageDirs) -> ProductConfig {
    let mut product = base(target, &format!("upupt {face} imgdef mismatch"));
    product.body_json = Some(imgdef_body());
    product.body_image_dir = Some(images.mismatch_dir.clone());
    product.body_image_field = Some("image".into());
    product.body_image_name_field = Some("filename".into());
    product.expect_checks = imgdef_checks(face, false);
    product.timeout_secs = Some(timeout_secs(face, "imgdef"));
    product
}

fn base(target: Target, name: &str) -> ProductConfig {
    ProductConfig {
        name: name.to_string(),
        product_id: target.product_id,
        path: target.path,
        method: "POST".into(),
        qps_limit: None,
        timeout_secs: None,
        query: BTreeMap::default(),
        headers: BTreeMap::default(),
        body_json: None,
        body_image_dir: None,
        body_image_field: None,
        body_image_name_field: None,
        expect_transport: "ok".into(),
        expect_checks: Vec::new(),
    }
}

fn timeout_secs(_face: &str, method: &str) -> u64 {
    // job_poll 已为 submit-only：不再为「同步等终态」留 90s；imgdef 仍可能较慢。
    if method == "imgdef" {
        90
    } else {
        30
    }
}

fn camel(face: &str) -> bool {
    face == "grpc"
}

fn posts_body(face: &str, invalid: bool) -> String {
    let body = if camel(face) {
        if invalid {
            json!({"keyword":"gRPC","userId":"6","page":"1","pageSize":"10","sort":"id_desc"})
        } else {
            json!({"keyword":"gRPC","userId":6,"page":1,"pageSize":10,"sort":"id_desc"})
        }
    } else if invalid {
        json!({"keyword":"gRPC","user_id":"6","page":"1","page_size":"10","sort":"id_desc"})
    } else {
        json!({"keyword":"gRPC","user_id":6,"page":1,"page_size":10,"sort":"id_desc"})
    };
    body.to_string()
}

fn photos_body() -> String {
    json!({"scene":"article_cover","width":400,"height":300,"format":"jpeg"}).to_string()
}

fn imgdef_body() -> String {
    json!({"threshold":10}).to_string()
}

fn posts_checks(face: &str) -> Vec<FieldCheck> {
    match face {
        "graphql" => vec![
            FieldCheck::eq("postsSearch.total", json!(1)),
            FieldCheck::eq("postsSearch.items.0.id", json!(18)),
        ],
        // protojson writes uint64 as a decimal string.
        "grpc" => vec![
            FieldCheck::eq("total", json!(1)),
            FieldCheck::eq("items.0.id", json!("18")),
        ],
        // job_poll submit-only：生产路径立即返回 job_id，不再断言业务终态。
        "job" => vec![FieldCheck::exists("job_id")],
        _ => vec![
            FieldCheck::eq("total", json!(1)),
            FieldCheck::eq("items.0.id", json!(18)),
        ],
    }
}

fn photos_checks(face: &str) -> Vec<FieldCheck> {
    if face == "job" {
        return vec![FieldCheck::exists("job_id")];
    }
    let path = match face {
        "graphql" => "photosRender.image.base64",
        "grpc" => "imageBytes",
        _ => "image.base64",
    };
    vec![FieldCheck::exists(path)]
}

fn imgdef_checks(face: &str, matched: bool) -> Vec<FieldCheck> {
    if face == "job" {
        return vec![FieldCheck::exists("job_id")];
    }
    let (matched_path, distance_path) = if face == "graphql" {
        ("imgdefCompare.matched", "imgdefCompare.distance")
    } else {
        ("matched", "distance")
    };
    let mut checks = vec![FieldCheck::eq(matched_path, json!(matched))];
    if !matched {
        checks.push(FieldCheck::eq(distance_path, json!(64)));
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;
    use std::io::Write;

    fn bind_text(images: bool) -> String {
        let mut text = String::from("[bind]\n");
        for face in FACES {
            for method in METHODS {
                let _ = writeln!(text, "{face}.{method} = \"pid-{face}-{method}\"");
            }
        }
        if images {
            text.push_str(
                "\n[images]\nmatch_dir = \"/tmp/match\"\nmismatch_dir = \"/tmp/mismatch\"\n",
            );
        }
        text
    }

    fn load(suite: Suite, images: bool) -> Vec<ProductConfig> {
        let mut file = tempfile();
        write!(file.1, "{}", bind_text(images)).unwrap();
        let raw = fs::read_to_string(&file.0).unwrap();
        let parsed: BindFile = toml::from_str(&raw).unwrap();
        let flat = flatten_bind(&parsed.bind).unwrap();
        expand(suite, &identity(&flat), &parsed).unwrap()
    }

    /// Channel code doubles as product id; path is `/<face>/<method>`.
    fn identity(flat: &BTreeMap<String, String>) -> BTreeMap<String, Target> {
        flat.iter()
            .map(|(key, code)| {
                let target = Target {
                    product_id: code.clone(),
                    path: format!("/{}", key.replace('.', "/")),
                };
                (key.clone(), target)
            })
            .collect()
    }

    fn tempfile() -> (std::path::PathBuf, fs::File) {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "upupt-bind-{}-{seq}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = fs::File::create(&path).unwrap();
        (path, file)
    }

    #[test]
    fn pressure_covers_nine_faces_without_negative_images() {
        let products = load(Suite::Pressure, false);
        assert_eq!(products.len(), 27);
        assert!(products.iter().all(|p| p.expect_transport == "ok"));
        assert!(products.iter().all(|p| {
            p.body_image_dir
                .as_deref()
                .is_none_or(|dir| !dir.contains("nomatch"))
        }));
        let grpc = products
            .iter()
            .find(|p| p.name == "upupt grpc posts")
            .unwrap();
        assert_eq!(grpc.path, "/grpc/posts");
        assert!(grpc.body_json.as_deref().unwrap().contains("userId"));
        assert!(!grpc.body_json.as_deref().unwrap().contains("user_id"));
        assert!(
            grpc.expect_checks
                .iter()
                .any(|c| c.path == "items.0.id" && c.eq == Some(json!("18")))
        );
        let soap = products
            .iter()
            .find(|p| p.name == "upupt soap posts")
            .unwrap();
        assert!(
            soap.expect_checks
                .iter()
                .any(|c| c.path == "items.0.id" && c.eq == Some(json!(18)))
        );
        let soap_photo = products
            .iter()
            .find(|p| p.name == "upupt soap photos")
            .unwrap();
        assert!(
            soap_photo
                .expect_checks
                .iter()
                .any(|c| c.path == "image.base64" && c.eq.is_none())
        );
        let rest = products
            .iter()
            .find(|p| p.name == "upupt rest imgdef")
            .unwrap();
        assert_eq!(rest.body_image_dir.as_deref(), Some("../imgs"));
        assert_eq!(rest.body_image_name_field.as_deref(), Some("filename"));
        let job = products
            .iter()
            .find(|p| p.name == "upupt job posts")
            .unwrap();
        assert_eq!(job.timeout_secs, Some(30)); // submit-only，不再等终态
        assert!(
            job.expect_checks
                .iter()
                .any(|c| c.path == "job_id" && c.eq.is_none()),
            "job face must assert job_id exists, not terminal business payload"
        );
    }

    #[test]
    fn coverage_adds_mismatch_and_invalid_per_face() {
        let products = load(Suite::Coverage, true);
        assert_eq!(products.len(), 45);
        let mismatch = products
            .iter()
            .find(|p| p.name == "upupt rest imgdef mismatch")
            .unwrap();
        assert_eq!(mismatch.body_image_dir.as_deref(), Some("/tmp/mismatch"));
        assert_eq!(mismatch.product_id, "pid-rest-imgdef");
        assert!(
            mismatch
                .expect_checks
                .iter()
                .any(|c| c.path == "matched" && c.eq == Some(json!(false)))
        );
        let invalid = products
            .iter()
            .find(|p| p.name == "upupt graphql posts invalid")
            .unwrap();
        assert_eq!(invalid.expect_transport, "reject");
        assert!(
            invalid.expect_checks.is_empty(),
            "expect_checks should be empty"
        );
        assert_eq!(invalid.product_id, "pid-graphql-posts");
    }

    #[test]
    fn rest_suite_does_not_require_other_faces() {
        let mut text = String::from("[bind]\n");
        for method in METHODS {
            let _ = writeln!(text, "rest.{method} = \"pid-rest-{method}\"");
        }
        let parsed: BindFile = toml::from_str(&text).unwrap();
        let flat = flatten_bind(&parsed.bind).unwrap();
        let products = expand(Suite::Rest, &identity(&flat), &parsed).unwrap();
        assert_eq!(products.len(), 3);
        assert!(products.iter().all(|p| p.path.starts_with('/')));
    }

    #[test]
    fn missing_or_unknown_bind_key_fails() {
        let parsed: BindFile = toml::from_str("[bind]\nrest.posts = \"x\"\n").unwrap();
        let flat = flatten_bind(&parsed.bind).unwrap();
        let err = expand(Suite::Rest, &identity(&flat), &parsed)
            .unwrap_err()
            .to_string();
        assert!(err.contains("rest.photos"), "{err}");

        let mut text = bind_text(false);
        text.push_str("extra.posts = \"nope\"\n");
        let parsed: BindFile = toml::from_str(&text).unwrap();
        let flat = flatten_bind(&parsed.bind).unwrap();
        let err = resolve_products(&flat, &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown bind key"), "{err}");
    }

    const BOX_FACES: &[&str] = &["oauth", "sign", "rest", "jsonrpc", "graphql", "soap", "job"];

    fn box_text(extra_head: &str) -> String {
        let mut text = String::from(extra_head);
        text.push_str("\n[bind]\n");
        for face in BOX_FACES {
            for method in METHODS {
                let _ = writeln!(text, "{face}.{method} = \"pid-{face}-{method}\"");
            }
        }
        text
    }

    fn expand_text(suite: Suite, text: &str) -> Result<Vec<ProductConfig>> {
        let parsed: BindFile = toml::from_str(text)?;
        let flat = flatten_bind(&parsed.bind)?;
        expand(suite, &identity(&flat), &parsed)
    }

    const BOX_HEAD: &str =
        "faces = [\"oauth\", \"sign\", \"rest\", \"jsonrpc\", \"graphql\", \"soap\", \"job\"]\n";

    #[test]
    fn faces_subset_skips_unselected() {
        let products = expand_text(Suite::Pressure, &box_text(BOX_HEAD)).unwrap();
        assert_eq!(products.len(), 21);
        assert!(
            products
                .iter()
                .all(|p| !p.path.starts_with("/ws/") && !p.path.starts_with("/grpc/"))
        );
        assert!(products.iter().all(|p| p.qps_limit.is_none()));

        let err = expand_text(Suite::Pressure, &box_text(""))
            .unwrap_err()
            .to_string();
        assert!(err.contains("bind file missing"), "{err}");
    }

    #[test]
    fn faces_unknown_or_empty_fails() {
        let err = expand_text(Suite::Pressure, &box_text("faces = [\"rest\", \"nope\"]\n"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown face 'nope'"), "{err}");

        let err = expand_text(Suite::Pressure, &box_text("faces = []\n"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be empty"), "{err}");

        let err = expand_text(Suite::Rest, &box_text("faces = [\"soap\"]\n"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("selects nothing"), "{err}");
    }

    #[test]
    fn method_qps_applied() {
        let head = format!("{BOX_HEAD}\n[qps]\nposts = 45\nphotos = 9\nimgdef = 8\n");
        let products = expand_text(Suite::Pressure, &box_text(&head)).unwrap();
        for product in &products {
            let want = if product.name.ends_with("posts") {
                45
            } else if product.name.ends_with("photos") {
                9
            } else {
                8
            };
            assert_eq!(product.qps_limit, Some(want), "{}", product.name);
        }

        let products = expand_text(Suite::Coverage, &box_text(&head)).unwrap();
        assert_eq!(products.len(), 35);
        let invalid = products
            .iter()
            .find(|p| p.name == "upupt soap posts invalid")
            .unwrap();
        assert_eq!(invalid.qps_limit, Some(45));
        let mismatch = products
            .iter()
            .find(|p| p.name == "upupt job imgdef mismatch")
            .unwrap();
        assert_eq!(mismatch.qps_limit, Some(8));
    }

    #[test]
    fn method_qps_zero_or_unknown_fails() {
        let head = format!("{BOX_HEAD}\n[qps]\nphotos = 0\n");
        let err = expand_text(Suite::Pressure, &box_text(&head))
            .unwrap_err()
            .to_string();
        assert!(err.contains("qps.photos"), "{err}");

        let head = format!("{BOX_HEAD}\n[qps]\nvideos = 3\n");
        assert!(expand_text(Suite::Pressure, &box_text(&head)).is_err());
    }

    fn target(id: &str, path: &str) -> Target {
        Target {
            product_id: id.into(),
            path: path.into(),
        }
    }

    #[test]
    fn product_map_resolves_channel_codes() {
        let bind = BTreeMap::from([
            ("rest.posts".to_string(), "CH-POSTS".to_string()),
            ("rest.photos".to_string(), " CH-PHOTOS ".to_string()),
        ]);
        let map = BTreeMap::from([
            ("CH-POSTS".to_string(), target(" P1 ", "/up/posts")),
            ("CH-PHOTOS".to_string(), target("P2", "/up/photos")),
        ]);
        let resolved = resolve_products(&bind, &map).unwrap();
        assert_eq!(resolved["rest.posts"].product_id, "P1");
        assert_eq!(resolved["rest.posts"].path, "/up/posts");
        assert_eq!(resolved["rest.photos"].product_id, "P2");
    }

    #[test]
    fn product_map_reports_every_missing_code() {
        let bind = BTreeMap::from([
            ("rest.posts".to_string(), "CH-POSTS".to_string()),
            ("rest.photos".to_string(), "CH-PHOTOS".to_string()),
            ("rest.imgdef".to_string(), "CH-IMGDEF".to_string()),
            ("job.posts".to_string(), "CH-JOB".to_string()),
        ]);
        let map = BTreeMap::from([
            ("CH-POSTS".to_string(), target("P1", "/up/posts")),
            ("CH-IMGDEF".to_string(), target(" ", "/up/imgdef")),
            ("CH-JOB".to_string(), target("P4", "relative")),
        ]);
        let err = resolve_products(&bind, &map).unwrap_err().to_string();
        assert!(err.contains("rest.photos=CH-PHOTOS"), "{err}");
        assert!(err.contains("rest.imgdef=CH-IMGDEF"), "{err}");
        assert!(err.contains("job.posts=CH-JOB"), "{err}");
        assert!(!err.contains("CH-POSTS"), "{err}");
    }

    #[test]
    fn load_cases_reads_bind_and_product_map() {
        let mut bind = tempfile();
        write!(bind.1, "{}", box_text(BOX_HEAD)).unwrap();
        let mut map = tempfile();
        let mut text = String::new();
        for face in BOX_FACES {
            for method in METHODS {
                let _ = writeln!(
                    text,
                    "[products.\"pid-{face}-{method}\"]\nproduct_id = \"prod-{face}-{method}\"\npath = \"/up/{face}/{method}\""
                );
            }
        }
        write!(map.1, "{text}").unwrap();
        let products = load_cases("upupt", "pressure", &bind.0, &map.0).unwrap();
        let _ = fs::remove_file(&bind.0);
        let _ = fs::remove_file(&map.0);
        assert_eq!(products.len(), 21);
        assert!(products.iter().all(|p| p.product_id.starts_with("prod-")));
        let rest = products
            .iter()
            .find(|p| p.name == "upupt rest posts")
            .unwrap();
        assert_eq!(rest.path, "/up/rest/posts");
    }
}
