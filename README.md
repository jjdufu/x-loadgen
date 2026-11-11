# x-loadgen

通用网关压测工具：模拟**租户客户**访问 Mazbri Gateway（非管理后台流量）。  
一切目标、租户、产品、流量模型、进程布局均由 **TOML 配置文件**驱动；二进制内**不含**任何业务场景（如 upupt / ENV-SC-SM4）硬编码。

- 版本：见仓库根 `Cargo.toml` 的 `[workspace.package]`
- 布局：Cargo workspace，成员 crate 位于 `x-loadgen-bin/x-loadgen`，二进制名 `x-loadgen`
- SDK：path 依赖 `../mazbri-max/mazbri-libs/mazbri-sdk`（`&self` 并发安全、每请求独立 nonce、内部 token 刷新；经同目录下的 `mazbri-core` / `mazbri-crypto`）

## 能力概览

| 能力 | 配置位置 |
|------|----------|
| 网关地址 / 可选 mTLS | `[gateway]` |
| 多租户（secrets 文件或内联）+ 权重混流 | `[[customers]]` |
| 加密模式 `none` / `request` / `full`（套件/算法按 SDK） | 各客户 `[tenant]` |
| 产品 method/path/body 模板或文件 + 变量替换 | `[[products]]` |
| channel → product_id 运行时映射 | `[resolve].product_map` |
| open-loop QPS / closed-loop 并发、总请求或时长、warmup/ramp | `[traffic]` |
| 多进程 + 每进程并发；QPS/请求按 total 或 per_process 切分 | `[cluster]` |
| HTML/JSON 报告 | `[report]` |
| 可选 ClickHouse 平台侧耗时补齐（按 request_id / 时间窗） | `[enrichment.clickhouse]` |

## 快速开始

```bash
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo run -p x-loadgen -- \
  --config x-loadgen-bin/x-loadgen/examples/generic.example.toml
```

常用覆盖：

```bash
x-loadgen --config path.toml \
  --qps 50 --concurrency 16 --requests 1000 \
  --processes 5 \
  --only-product face \
  --report-dir reports/run1
```

- `--processes N`（或配置 `cluster.processes`）>1 时：主进程拉起 N 个 worker（`--worker-index` / `--worker-count`），结束后汇总到 `<report.dir>/<run_id>/data/cluster-summary.json`（各 worker 报告在 `<report.dir>/<run_id>-wNN/`）。
- `--token-only`：仅对每个客户拉 token，不发业务请求。
- `keygen` 子命令：生成请求绑定密钥对（与压测无关）。

## 配置说明

完整注释示例见：

- `x-loadgen-bin/x-loadgen/examples/generic.example.toml` — 通用模板
- `x-loadgen-bin/x-loadgen/examples/env-sc-sm4.lab.toml` — **「仅联调使用」** 的 ENV-SC-SM4 样例（占位路径，需换成真实 secrets / product_map）

### 客户 secrets 文件

```toml
[tenant]
api_key = "..."
encryption_mode = "full"          # none | request | full
tenant_id = "..."
tenant_key_base64 = "..."
key_version = "v1"
suite = "suite/tenant-default-2026-11"
# data_alg / mac_alg / require_signature / allow_insecure 按需

# [tenant.binding]
# kid = "..."
# alg = "ed25519"
# private_key_pem = "keys/binding.pem"
```

### Body 模板占位符

`body_json` / `body_file` 支持：

- `{{uuid}}` `{{seq}}` `{{seq1}}` `{{iso8601}}`
- `{{random_int:MIN:MAX}}` `{{random_hex:N}}`

### 并发模型

- **单进程内**：加密传输使用共享 `MazbriClient`（`&self` 调用），**不再**在整段 RPC 上持有 `Mutex`；可真正并行打满 `traffic.concurrency`。
- **多进程**：由 `[cluster].processes` 拉起；配合多客户权重做客户侧混流。

### 平台指标补齐（可选）

```toml
[enrichment.clickhouse]
enabled = true
sql_file = "enrichment/platform_latency.sql"
# SQL 内可用 {{from_ms}} {{to_ms}} {{request_ids_csv}}
```

压测结束后执行一次，结果写入报告目录 `data/platform-enrichment.json`（可改 `output`）。

## 开发

```bash
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

box 上直连 crates.io（sparse），不添加项目级 `.cargo` 代理。工具链见仓库根 `rust-toolchain.toml`（与 mazbri CI 的 `RUST_TOOLCHAIN=1.99.1` 一致）。

## 兼容性说明

旧版 `--preset upupt` / 内置 case 集已移除。请把场景写成普通 `[[products]]` + `[resolve].product_map` + `[[customers]]` 配置（参见 `x-loadgen-bin/x-loadgen/examples/env-sc-sm4.lab.toml`）。
