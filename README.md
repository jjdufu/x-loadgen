# x-loadgen

网关压测工具。通过 Web 页面或 HTTP API 配置目标与模板并执行任务。命令行只提供基础功能，不能用来发起压测。

仓库：<https://github.com/jjdufu/x-loadgen>。自包含的独立项目，不依赖仓库外的任何路径。

- 版本：仓库根 `Cargo.toml` 的 `[workspace.package]`，由发版流程自动维护（见「CI 与发版」）
- 镜像：Docker Hub `dex2all/x-loadgen:<版本>`（linux/amd64 + linux/arm64），也可以在本地用仓库根 `Dockerfile` 构建
- 布局：Cargo workspace
  - `x-loadgen-bin/x-loadgen`：守护进程与库，二进制名 `x-loadgen`
  - `x-loadgen-bin/x-loadgen-mockgw`：基于 `x-loadgen-core` 的 mock mazbri 网关，供测试与端到端联调，不进入产品镜像
  - `x-loadgen-libs/x-loadgen-crypto`：算法闭集、注册表与软件密码学原语
  - `x-loadgen-libs/x-loadgen-core`：mazbri 传输信封与请求绑定签名编解码
  - `x-loadgen-libs/x-loadgen-sdk`：mazbri 网关客户端合同（换 token、密封请求、绑定头、解码响应）
- 前端：`x-loadgen-bin/x-loadgen/web`（Vue 3），发布时嵌入二进制
- 依赖全部来自 crates.io（sparse），不添加项目级 `.cargo` 代理。

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

在仓库根：

```bash
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

工具链见 `rust-toolchain.toml`（1.99.1）。前端在 `x-loadgen-bin/x-loadgen/web`：`pnpm lint`、`pnpm typecheck`、`pnpm build`。未构建前端时，debug 编译可以不嵌入页面；release 构建必须先有 `web/dist`。

## 镜像

### 拉取发版镜像

```bash
docker pull dex2all/x-loadgen:<版本>      # 例如 0.1.0；latest 指向最近一次发版
```

镜像同时提供 linux/amd64 与 linux/arm64。`deploy/config/.env.example` 默认就是 `X_LOADGEN_IMAGE=dex2all/x-loadgen`，`deploy/scripts/start.sh` 发现本机没有镜像时会自动拉取。

### 本地构建

构建上下文是仓库根目录，`Dockerfile` 分三段：`node` 构建前端，`debian:trixie` 按 `rust-toolchain.toml` 安装工具链并以 `cargo build --release --locked` 编译（嵌入前端），最后只把二进制放进 `debian:trixie-slim` 运行镜像。GitHub runner 上的发版构建用的就是这份 `Dockerfile`，不依赖本机的任何额外文件。

```bash
docker build --tag x-loadgen:<版本> .
# 或者用 buildx 指定平台
docker buildx build --platform linux/amd64 --load --tag x-loadgen:<版本> .
```

`.dockerignore` 排除 `.git`、`.github`、`target`、`node_modules`、`web/dist`、`deploy`、Markdown 文档和数据文件，前端产物在镜像内重新构建。本地构建的镜像用于部署时，在 `deploy/config/.env` 里设 `X_LOADGEN_IMAGE=x-loadgen`。

不能访问 Docker Hub 的主机用 `docker save` / `docker load` 导入：

```bash
docker save dex2all/x-loadgen:<版本> | gzip > x-loadgen-<版本>.tar.gz
# 在目标主机
gunzip -c x-loadgen-<版本>.tar.gz | docker load
```

## CI 与发版

工作流在 `.github/workflows/`，脚本在 `.github/scripts/`，action 全部按提交 SHA 固定版本。

### CI（`ci.yml`）

push 到 `main`、任何 PR 和手动触发时运行；同一分支或 PR 的新运行会取消旧运行。

| 任务 | 内容 |
| --- | --- |
| Scripts and workflows lint | `shellcheck` 检查仓库内全部 `*.sh`，`actionlint` 检查工作流，执行发版脚本自测 `.github/scripts/release-test.sh` |
| Frontend lint, typecheck, build | `pnpm install --frozen-lockfile`、`pnpm lint`、`pnpm typecheck`、`pnpm build`，把 `web/dist` 交给 Rust 任务 |
| Rust fmt, clippy -D warnings, test | 按 `rust-toolchain.toml` 安装工具链；`cargo fmt --all --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo test --locked --workspace`，编译时嵌入真实的前端产物 |

Node、pnpm 走 `setup-node` 的 pnpm 缓存，cargo 走 `Swatinem/rust-cache`（只在 `main` 保存）。

### 发版（`release.yml`）

`main` 上的 CI 成功后自动运行，不需要 release PR，也不需要手动打 tag。是否发版、发哪个版本只由提交信息决定：

| 自上一个 `vX.Y.Z` tag 以来的提交 | 0.x 阶段 | 1.0 之后 |
| --- | --- | --- |
| 破坏性变更：`feat!:`、`fix(scope)!:`，或正文含 `BREAKING CHANGE:` | 升 minor（0.1.3 → 0.2.0） | 升 major |
| `feat:` / `feat(scope):` | 升 patch（0.1.0 → 0.1.1） | 升 minor |
| `fix:`、`perf:`、`revert:` | 升 patch | 升 patch |
| 只有 `docs`、`chore`、`ci`、`refactor`、`test`、`style`、`build` 等 | 不发版 | 不发版 |

- 规则与 mazbri 的 release-please 配置一致（`bump-minor-pre-major`、`bump-patch-for-minor-pre-major`）。取最高的一级，一次发版只升一级。
- 提交标题格式：`类型(可选范围)!: 描述`，类型小写，冒号后有一个空格，描述可以写中文，例如 `feat(engine): 支持按租户限速`、`fix: 修复多进程任务不结束`。
- 仓库还没有任何 `vX.Y.Z` tag 时，按 `Cargo.toml` 的当前版本做首次发版，与提交类型无关。
- 发版流程：`.github/scripts/release.sh` 计算版本，改写 `Cargo.toml`（workspace 版本和内部依赖版本）、`Cargo.lock`、`web/package.json`、`deploy/config/.env.example`、`deploy/README.md`，把本次的 Features / Bug Fixes 等段落置顶写入 `CHANGELOG.md`；以 `github-actions[bot]` 提交 `chore(release): vX.Y.Z [skip ci]`，打附注 tag `vX.Y.Z` 并推回 `main`；amd64 与 arm64 原生 runner 分别构建仓库根 `Dockerfile`，合并推送 `dex2all/x-loadgen:X.Y.Z` 与 `:latest`，校验两种架构都在；最后创建 GitHub Release，内容为 CHANGELOG 段落和 `docker pull` 命令。
- release 提交不会再触发发版：用 `GITHUB_TOKEN` 推送的提交和 tag 不触发任何 workflow，提交信息又带 `[skip ci]`，`release.sh` 也不把 `chore(release)` 计入下一次发版。
- CI 跑完时如果 `main` 已经有更新的提交，本次不发版，交给新提交对应的那次运行。
- 某个任务失败（例如推送 Docker Hub 超时）时，在 Actions 页面点 “Re-run failed jobs”；已完成的 tag 与版本号会被沿用，不会重复升版本。
- 需要的仓库 secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`（对 `dex2all/x-loadgen` 有推送权限）。

本地预览下一次发版：

```bash
.github/scripts/release.sh plan          # 输出 release / version / previous / bump，发版说明写到 .git/release-notes.md
.github/scripts/release-test.sh          # 用临时仓库验证各种提交组合
```

## 用 deploy/ 部署

`deploy/` 是独立的 Compose 工程，布局与 mazbri 中间件的单组件工程一致（`config/compose.yml`、`config/.env`、`scripts/*.sh`、`lib/common.sh`、`common/base.yml`，运行期数据在 `data/ logs/ backups/`）。脚本由自身位置推导路径，目录可以改名或整体拷到任意位置，在任意工作目录下调用。

```bash
deploy/scripts/start.sh      # 首次从 config/.env.example 生成 config/.env；等待 healthy，打印访问地址和 admin.passwd 路径
deploy/scripts/status.sh
deploy/scripts/backup.sh     # 一致性备份 data/ 到 deploy/backups/
deploy/scripts/stop.sh       # 或 down.sh（删容器与网络，数据保留）
```

默认桥接网络，容器内监听 `0.0.0.0:8757`，宿主机发布 `0.0.0.0:8757`（`config/.env` 里 `X_LOADGEN_BIND=127.0.0.1` 则只开本机）。容器以执行者的 uid:gid 运行，数据目录归执行者。联调可设 `X_LOADGEN_NO_AUTH=1`。端口、资源、host 网络切换、备份恢复和镜像分发见 `deploy/README.md`。本工程独立启动，不写入 mazbri-max 的编排。

## 登录与改密

默认鉴权开启时，首次启动会在数据目录（容器内 `/data`，宿主机 `deploy/data/`）的 `admin.passwd` 写入初始口令（文件模式 600）。打开页面进入登录页，提交该口令后必须立即改密：新口令至少 16 个字符，且不能与旧口令相同。改密完成前，业务 API 返回 403（`password_change_required`）。

改密后 `admin.passwd` 被删除，服务重启也不会再生成。重置口令：

```bash
deploy/scripts/reset-passwd.sh   # 在容器内执行 x-loadgen reset-passwd，只打印口令文件路径
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

健康检查：`GET /health` 或 `x-loadgen health`，成功时返回 `{"ok":true,"version":"<版本>"}`。

## 在页面里配置并启动任务

登录（或 `--no-auth`）之后：

1. 打开「目标」，新建目标。选择协议（mazbri 网关 / 普通 HTTP），填写名称和 `gateway_base_url`，再配置租户与产品。mazbri 的加密与签名字段见下一节。
2. 打开「模板」，新建模板并绑定目标。单次任务填写 `requests` 或 `duration_secs`；循环任务打开 loop，填写 `rounds` 与 `loop_interval_secs`。
3. 打开「任务」，选择模板和模式（`once` / `loop`）后启动。若已有正在运行的任务，页面会先确认再启动；API 不会只因为另有任务在跑就返回 409。
4. 任务详情页可看实时统计、调整流量、停止或续跑。报告页查看脱敏摘要，并可下载 JSON / HTML。

`gateway_base_url` 必须是**容器能够访问**的地址：IP、公网 URL，或 DNS 可解析的域名加端口。不要填写 `http://127.0.0.1:8888` 来指向宿主机上的网关（那是容器自己的回环）。指向宿主机上的监听时，使用宿主机在该桥接网络上的地址，或该服务对容器可见的地址。

对应的 HTTP API（均在 `/api/v1` 下，需登录 cookie，除非 `--no-auth`）：

- `POST /targets`、`GET /targets`、`GET|PUT|DELETE /targets/{id}` 以及租户、产品子资源
- `POST /targets/{id}/tenants/{tid}/binding-keypair`（`{"alg":"sm2-sign","kid":"..."}`）为租户生成签名密钥对，只返回公钥
- `POST /templates`、`GET /templates`、`GET|PUT|DELETE /templates/{id}`
- `POST /runs`（`{"template_id":"...","mode":"once"|"loop"}`）启动任务
- `GET /runs`、`GET /runs/{id}`、`GET /runs/{id}/stats`
- `POST /runs/{id}/stop`、`POST /runs/{id}/resume`
- `GET /runs/{id}/report`、`GET /runs/{id}/report.json`、`GET /runs/{id}/report.html`

页面路由：`/login`、`/password`、`/targets`、`/templates`、`/runs`、`/runs/:id`、`/runs/:id/report`、`/settings`。

## 接入 mazbri 网关（加密与签名）

协议选「mazbri 网关」时，x-loadgen 按 mazbri 官方客户端合同发请求，加解密逻辑来自仓库内的 `x-loadgen-libs`，报文格式与网关一致：

1. 换 token：`POST {gateway_base_url}/auth/token`，body 为 `{"api_key":"...","grant_type":"client_credentials"}`。token 按进程、按租户缓存，过期前（`expires_in` 减去刷新提前量）单飞刷新；网关返回 401 时作废缓存并重新换取。
2. 调用：`POST {gateway_base_url}/api/v1/gateway`，带 `Authorization: Bearer <token>`。请求体为 `GatewayRequest`（`product_id`、`path`、`method`、`query`、`headers`、`body`），产品里的 `{{seq}}` 在渲染时替换为请求序号。
3. 请求绑定（可选）：对 `METHOD\npath\ntimestamp\nnonce\nbody` 签名，附带 `x-mazbri-timestamp`、`x-mazbri-nonce`、`x-mazbri-signature`、`x-mazbri-signature-input`、`x-mazbri-key-id`。
4. 加密：`none` 发明文；`request` 只密封请求，响应按明文解析；`full` 请求和响应都加密，响应解密并校验 MAC 后才参与统计。

耗时、状态统计和业务校验（`expect_transport`、`expect_checks_json`）都基于解密后的响应。

### 支持矩阵

| 类别 | 取值 |
| --- | --- |
| 加密模式 | `none`、`request`、`full` |
| 套件 | `suite/tenant-default-2026-11`（AES-256-GCM）、`suite/gm-sm4-hmac-sm3`（SM4-CBC + HMAC-SM3）、`suite/tenant-chacha20`（ChaCha20-Poly1305） |
| 自定义 AEAD（`data_alg`） | `aes-128-gcm`、`aes-192-gcm`、`aes-256-gcm`、`chacha20-poly1305` |
| 自定义分组密码（`data_alg`，需配 `mac_alg`） | `aes-128-cbc`、`aes-256-cbc`、`aes-128-ctr`、`aes-256-ctr`、`aes-128-cfb`、`aes-256-cfb`、`sm4-cbc-pkcs7`、`sm4-ctr` |
| MAC（`mac_alg`） | `hmac-sha256`、`hmac-sha384`、`hmac-sha512`、`hmac-sm3` |
| legacy（需打开「允许不安全算法」） | `aes-128-ecb`、`aes-256-ecb`、`sm4-ecb-pkcs7`、`des-cbc-pkcs5`、`hmac-md5` |
| 请求绑定 | `hmac-sha256`、`hmac-sha384`、`hmac-sha512`、`hmac-sm3`；`rsa-pkcs1v15-sign-sha256/384/512`；`ecdsa-p256-sha256`、`ecdsa-p384-sha384`；`ed25519`；`sm2-sign` |

页面上的「数据算法」「MAC 算法」「绑定算法」下拉框列出的就是可选闭集，保存时由 SDK 做完整校验（密钥长度、算法组合、legacy 开关、签名私钥可解析），不合法的配置不能保存。

### 在页面上完成配置

每个租户可填写：API Key、加密模式、租户 ID、密钥版本、租户密钥（标准 Base64）、套件或自定义 `data_alg`/`mac_alg`、信封签名要求、允许不安全算法；请求绑定填写算法、Key ID，以及 HMAC 密钥或 PKCS#8 PEM 私钥。

非对称绑定可以不准备私钥文件：先保存目标，填写 Key ID，点击「生成签名密钥对」。私钥写入本机 SQLite 的租户记录，页面只显示公钥，复制后到 mgmt 登记该 kid 即可。修改绑定算法或私钥后，已生成的公钥会被清空，需要重新生成或登记。

API Key、租户密钥、HMAC 密钥和私钥在目标详情里默认遮盖，可点眼睛图标显示或直接复制；目标列表、任务详情、报告和导出始终脱敏，公钥不属于敏感字段。

### 错误分类

| 分类 | 含义 |
| --- | --- |
| `token_refresh` | 换 token 失败，请求没有发出 |
| `transport` | 网络错误，或网关返回非 2xx（附网关错误信息） |
| `crypto` | 2xx 响应无法解密、MAC 校验失败或使用了未允许的算法 |
| `protocol` | 2xx 响应格式不符合合同，例如 `full` 模式下收到明文 |
| `business` / `unexpected_accept` | 解密后的响应未通过业务校验 |
| `config` | 租户配置在运行时无法建立会话 |

带错误分类的样本都计为失败。

### 普通 HTTP 目标

协议选「普通 HTTP」时直接请求「地址 + 产品路径」，租户 API Key（可选）以 `X-API-KEY` 头发送；此时租户只能使用 `none` 加密模式，不能填写加密或绑定字段。

## Mock 网关

`x-loadgen-mockgw` 按同一份合同实现 `/auth/token` 与 `/api/v1/gateway`：校验 token、请求绑定签名、打开请求信封，并把请求原样回显（`data.echo`），`full` 模式下加密响应。它用于自动化测试和本地端到端联调：

```bash
cargo run -p x-loadgen-mockgw -- --example > /tmp/mockgw.json   # 示例配置
cargo run -p x-loadgen-mockgw -- --listen 0.0.0.0:18080 --config /tmp/mockgw.json
```

配置字段与页面上的租户字段同名，另有 `binding_verify_key`（HMAC 密钥或签名公钥 PEM）和故障注入 `fault`（`none` / `tamper_response` / `plaintext_response`）。运行中可以用 `PUT /__mock/tenants` 替换租户配置（例如登记页面生成的公钥），`GET /__mock/stats` 查看已签发 token、已验签与已解封的计数，`GET /__mock/health` 做存活检查。

容器内的 x-loadgen 访问宿主机上的 mock 网关时，`gateway_base_url` 填宿主机的局域网 IP，或 `${STACK}-x-loadgen-net` 网络的网关地址，例如 `http://<宿主机IP>:18080`。
