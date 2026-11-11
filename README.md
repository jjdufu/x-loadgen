# x-loadgen

网关压测工具：通过 Web 页面与 HTTP API 配置目标、模板并执行任务。命令行只提供基础功能。

- 版本：见仓库根 `Cargo.toml` 的 `[workspace.package]`
- 布局：Cargo workspace，成员 crate 位于 `x-loadgen-bin/x-loadgen`，二进制名 `x-loadgen`
- SDK：path 依赖 `../mazbri-max/mazbri-libs/mazbri-sdk`（经同目录下的 `mazbri-core` / `mazbri-crypto`）

## 命令行

```text
x-loadgen serve [--listen 0.0.0.0:8757] [--data-dir ./data] [--no-auth] [--max-qps N] [--max-concurrency N]
x-loadgen reset-passwd [--data-dir ./data]
x-loadgen version
x-loadgen health [--url http://127.0.0.1:8757/health]
```

`serve` 对应环境变量（命令行优先）：`X_LOADGEN_LISTEN`、`X_LOADGEN_DATA_DIR`、`X_LOADGEN_NO_AUTH`（`1`/`true`）、`X_LOADGEN_MAX_QPS`、`X_LOADGEN_MAX_CONCURRENCY`。压测不能通过命令行发起。

当前步骤中，`serve` 与 `reset-passwd` 已解析参数；HTTP 监听、登录与压测引擎按后续步骤落地。`health` 在进程尚未监听时以非 0 退出。

## 开发

在仓库根、box 上直连 crates.io（sparse），不添加项目级 `.cargo`：

```bash
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

工具链见 `rust-toolchain.toml`（1.99.1）。
