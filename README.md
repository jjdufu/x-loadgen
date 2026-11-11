# x-loadgen

网关压测工具。通过 Web 页面或 HTTP API 配置目标与模板并执行任务。命令行只提供基础功能，不能用来发起压测。

仓库位于 `/workspace/rust-proj/x-loadgen`，与 `mazbri-max` 在 `rust-proj` 下并列。

- 版本：见仓库根 `Cargo.toml` 的 `[workspace.package]`（当前为 0.1.0）
- 布局：Cargo workspace，成员 crate 位于 `x-loadgen-bin/x-loadgen`，二进制名 `x-loadgen`
- 前端：`x-loadgen-bin/x-loadgen/web`（Vue 3），发布时嵌入二进制
- SDK：path 依赖 `../mazbri-max/mazbri-libs/mazbri-sdk` 与 `../mazbri-max/mazbri-libs/mazbri-core`（经同目录下的 `mazbri-crypto`）
- 编译本仓库需要并列的 `../mazbri-max/mazbri-libs`。在 box 上直连 crates.io（sparse），不添加项目级 `.cargo` 代理。

## 命令行

对外命令只有下面四个：

```text
x-loadgen serve [--listen 0.0.0.0:8757] [--data-dir ./data] [--no-auth] [--max-qps N] [--max-concurrency N]
x-loadgen reset-passwd [--data-dir ./data]
x-loadgen version
x-loadgen health [--url http://127.0.0.1:8757/health]
```

`serve` 对应环境变量（命令行优先）：`X_LOADGEN_LISTEN`、`X_LOADGEN_DATA_DIR`、`X_LOADGEN_NO_AUTH`（`1` / `true` / `yes` / `on`）、`X_LOADGEN_MAX_QPS`、`X_LOADGEN_MAX_CONCURRENCY`。空的上限环境变量视为未设置。

默认开启登录。口令写在数据目录下的 `admin.passwd`。`X_LOADGEN_NO_AUTH=1` 或 `--no-auth` 时无需登录即可打开页面和调用 API。`health` 在进程尚未监听时以非 0 退出。

凭据、目标、模板和压测参数都通过页面或 HTTP API 配置，不使用 toml 配置文件，也没有 `--config` 或压测子命令。

## 开发

在仓库根、box 上：

```bash
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

工具链见 `rust-toolchain.toml`（1.99.1）。前端在 `x-loadgen-bin/x-loadgen/web`：`pnpm lint`、`pnpm typecheck`、`pnpm build`。未构建前端时，debug 编译可以不嵌入页面；release 构建必须先有 `web/dist`。

## 在 box 上构建镜像

镜像名 `x-loadgen`，标签为版本号（当前为 `0.1.0`）。只在 box 本地构建。构建必须用 BuildKit 把并列的 `mazbri-libs` 作为额外 context 传入，以保持 `Cargo.toml` 中的 path 依赖。

```bash
cd /workspace/rust-proj/x-loadgen

export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
DOCKER_BUILDKIT=1 docker build \
  --file Dockerfile \
  --build-context libs=../mazbri-max/mazbri-libs \
  --tag x-loadgen:${X_LOADGEN_VERSION:-0.1.0} \
  --build-arg CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
  .
```

`.dockerignore` 会排除 `.cargo`、`target`、`node_modules`、`.git` 和数据目录。构建走 crates.io sparse。拷到其它主机使用 `docker save` / `docker load`：

```bash
docker save x-loadgen:0.1.0 | gzip > x-loadgen-0.1.0.tar.gz
# 在目标主机
gunzip -c x-loadgen-0.1.0.tar.gz | docker load
```

不要在 o50 上用本仓库 Dockerfile 构建。

## 用 deploy/ 启动 compose

默认桥接网络。容器内监听 `0.0.0.0:8757`，数据卷挂到容器内 `/data`。镜像以 tini 作为 PID 1。

```bash
cd /workspace/rust-proj/x-loadgen/deploy
cp env.example .env   # 按需修改
mkdir -p data && sudo chown 10001:10001 data
docker compose --env-file .env up -d
```

浏览器打开 `http://127.0.0.1:8757`。宿主机数据目录必须对 uid 10001 可写。`X_LOADGEN_HOST_BIND=127.0.0.1:` 时只有本机可访问 8757。联调可在 `.env` 里设 `X_LOADGEN_NO_AUTH=1`。

端口、数据目录、host 网络切换和镜像分发的细节见 `deploy/README.md`。本 compose 独立启动，不写入 mazbri-max 的编排。

停止（保留镜像）：

```bash
cd /workspace/rust-proj/x-loadgen/deploy
docker compose down
```

## 登录与改密

默认鉴权开启时，首次启动会在 `/data/admin.passwd` 写入初始口令（文件模式 600）。打开页面进入登录页，提交该口令后必须立即改密：新口令至少 16 个字符，且不能与旧口令相同。改密完成前，业务 API 返回 403（`password_change_required`）。

重置口令（容器内）：

```bash
docker compose exec x-loadgen x-loadgen reset-passwd --data-dir /data
```

会话 cookie 名为 `x_loadgen_session`（HttpOnly，SameSite=Lax）。用 curl 调用 API 时带上 cookie，例如：

```bash
curl -c /tmp/xlg.cookie -b /tmp/xlg.cookie \
  -H 'Content-Type: application/json' \
  -d '{"token":"<admin.passwd 或已改密的口令>"}' \
  http://127.0.0.1:8757/api/v1/auth/login

# 若 must_change_password 为 true：
curl -c /tmp/xlg.cookie -b /tmp/xlg.cookie \
  -H 'Content-Type: application/json' \
  -d '{"old_token":"<旧口令>","new_token":"<至少 16 位的新口令>"}' \
  http://127.0.0.1:8757/api/v1/auth/password
```

健康检查：`GET /health` 或 `x-loadgen health`，成功时返回 `{"ok":true,"version":"0.1.0"}`。

## 在页面里配置并启动任务

登录（或 `--no-auth`）之后：

1. 打开「目标」，新建目标。填写名称和 `gateway_base_url`，并按需配置租户与产品（路径、方法、权重、密钥等）。
2. 打开「模板」，新建模板并绑定目标。单次任务填写 `requests` 或 `duration_secs`；循环任务打开 loop，填写 `rounds` 与 `loop_interval_secs`。
3. 打开「任务」，选择模板和模式（`once` / `loop`）后启动。若已有正在运行的任务，页面会先确认再启动；API 不会只因为另有任务在跑就返回 409。
4. 任务详情页可看实时统计、调整流量、停止或续跑。报告页查看脱敏摘要，并可下载 JSON / HTML。

`gateway_base_url` 必须是**容器能够访问**的地址：IP、公网 URL，或 DNS 可解析的域名加端口。不要填写 `http://127.0.0.1:8888` 来指向宿主机上的网关（那是容器自己的回环）。指向宿主机上的监听时，使用宿主机在该桥接网络上的地址，或该服务对容器可见的地址。

对应的 HTTP API（均在 `/api/v1` 下，需登录 cookie，除非 `--no-auth`）：

- `POST /targets`、`GET /targets`、`GET|PUT|DELETE /targets/{id}` 以及租户、产品子资源
- `POST /templates`、`GET /templates`、`GET|PUT|DELETE /templates/{id}`
- `POST /runs`（`{"template_id":"...","mode":"once"|"loop"}`）启动任务
- `GET /runs`、`GET /runs/{id}`、`GET /runs/{id}/stats`
- `POST /runs/{id}/stop`、`POST /runs/{id}/resume`
- `GET /runs/{id}/report`、`GET /runs/{id}/report.json`、`GET /runs/{id}/report.html`

页面路由：`/login`、`/password`、`/targets`、`/templates`、`/runs`、`/runs/:id`、`/runs/:id/report`、`/settings`。
