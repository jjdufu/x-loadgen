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

`health` 在进程尚未监听时以非 0 退出。浏览器打开 `serve` 的监听地址即可使用页面；压测不能通过命令行发起。

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

## 在 box 上构建镜像

镜像名 `x-loadgen`，标签为版本号（当前为 `0.3.0`）。只在 box 本地构建，不要在 o50 上用本仓库 Dockerfile 构建。构建必须用 BuildKit 把并列的 `mazbri-libs` 作为额外 context 传入，以保持 `Cargo.toml` 中的 path 依赖 `../mazbri-max/mazbri-libs/{mazbri-core,mazbri-sdk}`。

```bash
cd /workspace/rust-proj/x-loadgen

export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
DOCKER_BUILDKIT=1 docker build \
  --file Dockerfile \
  --build-context libs=../mazbri-max/mazbri-libs \
  --tag x-loadgen:${X_LOADGEN_VERSION:-0.3.0} \
  --build-arg CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
  .
```

`.dockerignore` 会排除 `.cargo`、`target`、`node_modules`、`.git` 和数据目录，即使工作区误放了项目级代理配置也不会进入 context。构建走 crates.io sparse。分发到其它主机使用 `docker save` / `docker load`：

```bash
docker save x-loadgen:0.3.0 | gzip > x-loadgen-0.3.0.tar.gz
# 在目标主机
gunzip -c x-loadgen-0.3.0.tar.gz | docker load
```

独立 compose 的启动方式见 `deploy/README.md`（第 11 步）。
