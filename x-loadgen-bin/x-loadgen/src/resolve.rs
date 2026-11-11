//! Resolve `channel_code` → gateway `product_id` (+ optional path) from a map file.
//! Map format (TOML):
//!
//! ```toml
//! [products."CHANNEL-CODE"]
//! product_id = "8000000001"
//! path = "/vendor/foo"   # optional; fills product.path when empty
//! ```

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::config::ProductConfig;

#[derive(Debug, Deserialize)]
struct ProductMapFile {
    #[serde(default)]
    products: BTreeMap<String, ProductMapEntry>,
}

#[derive(Debug, Deserialize)]
struct ProductMapEntry {
    product_id: String,
    #[serde(default)]
    path: Option<String>,
}

/// Apply `[resolve].product_map` to products that set `channel_code`.
pub fn apply_product_map(products: &mut [ProductConfig], map_path: &Path) -> Result<()> {
    let raw = fs::read_to_string(map_path)
        .with_context(|| format!("failed to read product map {}", map_path.display()))?;
    let file: ProductMapFile = toml::from_str(&raw)
        .with_context(|| format!("failed to parse product map {}", map_path.display()))?;
    let mut missing: Vec<String> = Vec::new();
    for product in products.iter_mut() {
        let code = product.channel_code.as_deref().map_or("", str::trim);
        if code.is_empty() {
            continue;
        }
        let Some(entry) = file.products.get(code) else {
            missing.push(format!("{name} channel_code={code}", name = product.name));
            continue;
        };
        let pid = entry.product_id.trim();
        if pid.is_empty() {
            missing.push(format!(
                "{name} channel_code={code} (empty product_id)",
                name = product.name
            ));
            continue;
        }
        product.product_id = pid.to_owned();
        if product.path.trim().is_empty() {
            let path = entry.path.as_deref().map_or("", str::trim);
            if path.is_empty() || !path.starts_with('/') {
                missing.push(format!(
                    "{name} channel_code={code} (map path missing or not absolute)",
                    name = product.name
                ));
                continue;
            }
            product.path = path.to_owned();
        }
    }
    if !missing.is_empty() {
        bail!(
            "product map {} missing or invalid entries:\n  - {}",
            map_path.display(),
            missing.join("\n  - ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_channel_code_and_path() {
        let name = format!(
            "loadgen-map-{}-{}.toml",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        {
            let mut f = fs::File::create(&path).unwrap();
            write!(
                f,
                r#"
[products."CH-A"]
product_id = " 8001 "
path = "/a"
"#
            )
            .unwrap();
        }
        let mut products = vec![ProductConfig {
            name: "a".into(),
            product_id: String::new(),
            channel_code: Some("CH-A".into()),
            path: String::new(),
            method: "POST".into(),
            weight: 1,
            qps_limit: None,
            timeout_secs: None,
            query: BTreeMap::default(),
            headers: BTreeMap::default(),
            body_json: None,
            body_file: None,
            body_image_dir: None,
            body_image_field: None,
            body_image_name_field: None,
            expect_transport: "ok".into(),
            expect_checks: vec![],
        }];
        apply_product_map(&mut products, &path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(products[0].product_id, "8001");
        assert_eq!(products[0].path, "/a");
    }
}
