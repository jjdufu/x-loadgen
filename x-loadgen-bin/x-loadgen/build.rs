//! Embed `web/dist` into the binary when the frontend has been built.

use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=web/dist");
    println!("cargo:rerun-if-changed=web/dist/index.html");
    println!("cargo:rerun-if-env-changed=MAZBRI_SKIP_FRONTEND_EMBED");
    println!("cargo:rustc-check-cfg=cfg(frontend_embedded)");

    if skip_embed() {
        return;
    }

    let dist = Path::new("web/dist/index.html");
    if dist.is_file() {
        println!("cargo:rustc-cfg=frontend_embedded");
        return;
    }

    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        eprintln!(
            "error: 前端尚未构建，无法在 release 构建中嵌入页面。\n\
             请先执行：cd x-loadgen-bin/x-loadgen/web && pnpm install && pnpm build\n\
             纯后端测试可设置 MAZBRI_SKIP_FRONTEND_EMBED=1。"
        );
        std::process::exit(1);
    }
}

fn skip_embed() -> bool {
    matches!(
        env::var("MAZBRI_SKIP_FRONTEND_EMBED")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}
