# x-loadgen

双协议（通用 `http` + `mazbri` 网关）API 交互调试与高并发压测工作台，采用 **Shadcn/ui × Vercel Geist** 新瑞士极简黑白灰设计风格。通过内置 Web 页面或 HTTP API 配置集合（Collections）、请求（Requests）、环境与租户密钥池（Environments）及负载预设（Load Profiles），支持单次交互调试（`发送`）与多接口加权压测任务（`压测`），并内置离线可视化 OpenAPI 接口规范文档（`/docs`）。命令行只提供服务启动与基础管理功能，不能直接发起压测。

仓库：<https://github.com/jjdufu/x-loadgen>。自包含的独立项目，不依赖仓库外的任何路径。

- 版本：仓库根 `Cargo.toml` 的 `[workspace.package]`，由发版流程自动维护（见「CI 与发版」）
- 镜像：Docker Hub `dex2all/x-loadgen:<版本>`（linux/amd64 + linux/arm64），也可以在本地用仓库根 `Dockerfile` 构建
- 布局：Cargo workspace
  - `x-loadgen-bin/x-loadgen`：守护进程与库，二进制名 `x-loadgen`
  - `x-loadgen-bin/x-loadgen-mockgw`：基于 `x-loadgen-core` 的 mock mazbri 网关，仅供 `cargo test` 与端到端联调自测，不进入产品镜像或生产二进制
  - `x-loadgen-libs/x-loadgen-crypto`：算法闭集、注册表与软件密码学原语
  - `x-loadgen-libs/x-loadgen-core`：mazbri 传输信封与请求绑定签名编解码
  - `x-loadgen-libs/x-loadgen-sdk`：mazbri 网关客户端合同（换 token、密封请求、绑定头、解码响应）
- 前端：`x-loadgen-bin/x-loadgen/web`（Vue 3 + Shadcn/ui + Reka UI + Vercel Geist 可变字体，`Light` / `Black` 跟随系统明暗主题），发布时完整嵌入单二进制
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

凭据、集合、请求、环境（含租户池）和压测预设都通过页面或 HTTP API 配置，不使用 toml 配置文件，也没有 `--config` 或压测子命令。

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
- 提交信息（标题和正文）里不要出现 `[skip ci]`、`[ci skip]`、`[no ci]`、`[skip actions]`、`[actions skip]` 或 `skip-checks: true`：GitHub 会因此跳过这次 push 的全部 workflow，CI 和发版都不会运行。只有发版流程自己的 release 提交带 `[skip ci]`。
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

## 用 curl 调用 HTTP API

命令行本身不能发起压测。所有自动化（建环境与租户池、建集合与请求、单次 `probe` 调试、建压测预设、启动任务、调流量、取报告、判定）都通过 HTTP API 完成，推荐用 `curl` + `jq`。本节覆盖全部接口，例子均可直接复制。完整机器可读清单见无需认证的 [OpenAPI 文档](#openapi-文档)。

### 变量

在 shell 里先设定这些变量。后面所有例子都依赖它们。

```bash
# 服务地址（按实际部署修改）
XLG=http://127.0.0.1:8757

# 口令只放在权限 600 的文件里，不要写在命令行、不要 echo、不要 export 到可被 ps 看见的环境后再直接拼进 curl 参数
TOKEN_FILE=${TOKEN_FILE:-$HOME/.config/x-loadgen/token}
HDR=${HDR:-$HOME/.config/x-loadgen/auth.hdr}

# 用文件生成请求头，令牌不会进入 shell 历史
write_hdr() {
  umask 077
  mkdir -p "$(dirname "$HDR")"
  printf 'Authorization: Bearer %s\nContent-Type: application/json\n' "$(tr -d '\n' < "$TOKEN_FILE")" > "$HDR"
}

# 已认证的 JSON 请求；下载二进制时不要加 -H Content-Type，用下面的 xlg_get
xlg() { curl -fsS -H @"$HDR" "$@"; }
```

被压的网关地址用 `$GW` 表示，必须是 **x-loadgen 进程能够访问** 的 URL（容器部署时不要填 `127.0.0.1` 指向宿主机上的网关）。

### 使用提示

- JSON 请求体始终用单引号包起来，或 `--data-binary @file.json`，避免 shell 把 `$` 和 `"` 吃掉。
- 默认加上 `-fsS`：`-f` 在 4xx/5xx 时以状态 22 退出（便于脚本判断），`-sS` 安静但保留错误信息。需要看 HTTP 状态时用 `-w '%{http_code}'`，或临时去掉 `-f`。
- **永远不要**把口令写在命令行上（会进历史记录，也可能出现在 `ps` 里）。从文件读取；改密后立刻更新 `$TOKEN_FILE` 并重新执行 `write_hdr`。
- 需要排错时，把响应交给 `jq`：`xlg ... | jq .`。`jq` 在例子里用于抽取 `id` 等字段。
- 时间戳均为 Unix 毫秒。

### 鉴权

登录口令就是长期令牌：不自动过期，只在改密或 `reset-passwd` 之后失效。每个已认证请求用 `Authorization: Bearer <口令>` 或 Cookie `x_loadgen_token` 携带同一份口令。二者等价，脚本推荐 Bearer。

Cookie 属性：`HttpOnly`、`SameSite=Lax`、`Path=/`，无 `Secure`、无 `Max-Age`（浏览器会话 cookie）。登出只清 cookie，口令本身仍然有效。

#### 首次启动与必须改密

默认开启鉴权。首次启动会在数据目录下生成 `admin.passwd`（模式 600，64 位十六进制随机口令）。此时 `must_change_password` 为 true，除登录、改密、健康检查和 OpenAPI 外，业务接口返回 **403** `password_change_required`。

新口令规则：至少 **24** 个字符；仅限可打印 ASCII；不得包含空白、`"`、`;`、`,`、`\`；必须与旧口令不同。改密成功后 `admin.passwd` 被删除，重启也不会再生成。

```bash
# 把初始口令拷到 TOKEN_FILE（不要 cat 到终端）
umask 077
mkdir -p "$(dirname "$TOKEN_FILE")"
# 数据目录在容器内部是 /data，宿主机上一般是 deploy/data/admin.passwd
cp -f /path/to/data/admin.passwd "$TOKEN_FILE"

# 登录（响应里的 token 与口令相同，同时 Set-Cookie）
curl -fsS -X POST "$XLG/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  --data-binary @<(jq -n --rawfile t "$TOKEN_FILE" '{token: ($t | rtrimstr("\n"))}') \
  | jq '{must_change_password}'

# 改密：新口令先写入文件，再交给接口。成功后旧口令立即失效，Set-Cookie 换成新口令。
NEW_FILE=$(mktemp); chmod 600 "$NEW_FILE"
# 用自己保管的新口令覆盖 NEW_FILE，至少 24 字符，例如：openssl rand -hex 16 > "$NEW_FILE"
write_hdr   # 先用旧口令生成 $HDR
curl -fsS -X POST "$XLG/api/v1/auth/password" \
  -H @"$HDR" \
  --data-binary @<(jq -n --rawfile o "$TOKEN_FILE" --rawfile n "$NEW_FILE" \
      '{old_token: ($o | rtrimstr("\n")), new_token: ($n | rtrimstr("\n"))}') \
  | jq '{must_change_password, auth_required}'
mv -f "$NEW_FILE" "$TOKEN_FILE"
write_hdr
```

改密之后，用旧口令访问会得到 **401** `unauthorized`；用新口令（Bearer 或重新登录）立即可以访问。进程重启后同一口令仍然有效，不必再登录。

用 curl 做浏览器式 cookie 登录（把 cookie 存进 jar，后续 `-b` 带上，可以不写 Bearer）：

```bash
COOKIE_JAR=$HOME/.config/x-loadgen/cookies
umask 077
curl -fsS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -X POST "$XLG/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  --data-binary @<(jq -n --rawfile t "$TOKEN_FILE" '{token: ($t | rtrimstr("\n"))}') \
  | jq '{must_change_password}'
curl -fsS -c "$COOKIE_JAR" -b "$COOKIE_JAR" "$XLG/api/v1/auth/me" | jq .
curl -fsS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -X POST "$XLG/api/v1/auth/logout" | jq .
# 登出只清 cookie，口令仍有效；再用 Bearer 或重新登录即可
```

当前身份：

```bash
xlg "$XLG/api/v1/auth/me" | jq .
# {"auth_required":true,"must_change_password":false}
```

重置口令（另一个进程写入新的 `admin.passwd`，内存缓存会在下一次请求按 `updated_at` 失效，旧口令立即 401，并再次要求改密）：

```bash
# 容器部署
deploy/scripts/reset-passwd.sh          # 只打印口令文件路径，不打印内容
# 或直接
x-loadgen reset-passwd --data-dir /path/to/data
```

#### `--no-auth`

`X_LOADGEN_NO_AUTH=1` 或 `x-loadgen serve --no-auth` 时，所有 API 无需令牌，也不会生成 `admin.passwd`。此时调用登录或改密会返回 **400** `auth_disabled`。`GET /api/v1/auth/me` 返回 `{"auth_required":false,"must_change_password":false}`。下面的例子在 `--no-auth` 下只需把 `xlg` 换成 `curl -fsS -H 'Content-Type: application/json'`。

### 元信息与健康检查

`GET /health` 和 `GET /api/v1/openapi.json` **无需认证**。

```bash
curl -fsS "$XLG/health" | jq .
# {"ok":true,"version":"<版本>"}
# 也可以：x-loadgen health --url "$XLG/health"（失败以非 0 退出）

curl -fsS "$XLG/api/v1/openapi.json" | jq '{openapi, title: .info.title, paths: (.paths | keys | length)}'

xlg "$XLG/api/v1/meta" | jq .
# version、auth_required、max_qps、max_concurrency、running_qps、running_concurrency
```

### OpenAPI 文档

完整 OpenAPI 3.0 规范（公开接口无需认证，路径与运行中的路由一一对应）：

- **内置可视化接口规范视图**：在 Web 左侧导航栏点击「接口文档」（路由 `/docs`），即可离线浏览分组目录、搜索过滤、展开 `$ref` Schema、复制 `curl` 示例或一键载入工作台调试。
- **机器可读 JSON 清单**：

```text
GET $XLG/api/v1/openapi.json
```

可导入外部 API 工具，或在 `/docs` 页面右上角直接下载。

### 环境与租户密钥池（Environments & Tenants）

环境用于管理键值变量（支持 `<<var>>` 与 `{{var}}`）、默认 `gateway_base_url`、环境级 HTTP 鉴权、TLS 证书（同时支持直接粘贴 PEM 文本或服务器文件路径），以及 `mazbri` 加权多租户密钥池。系统内置一条 `is_global = true` 的全局环境（可编辑变量，不可删除）。

列表不返回密钥明文；详情（`GET /environments/{id}`）包含密钥字段；导出接口默认脱敏（`?include_secrets=true` 才导出真实密钥）。

```bash
# 创建一个包含变量和明文/加密租户池的环境
ENV_DOC=$(xlg -X POST "$XLG/api/v1/environments" --data-binary @- <<EOF | jq .
{
  "name": "staging-env",
  "gateway_base_url": "$GW",
  "variables": [
    {"key": "base_url", "value": "$GW", "secret": false, "active": true},
    {"key": "api_token", "value": "demo-token", "secret": true, "active": true}
  ],
  "tls": {"insecure_skip_verify": true},
  "note": "curl 手册演示环境",
  "tenants": [
    {"name": "plain-tenant", "weight": 1, "api_key": "demo-key", "encryption_mode": "none"}
  ]
}
EOF
)
ENV_ID=$(echo "$ENV_DOC" | jq -r .id)
echo "$ENV_DOC" | jq '{id, name, gateway_base_url, tenants: [.tenants[].name]}'

xlg "$XLG/api/v1/environments" | jq '[.[] | {id, name, is_global, variable_count, tenant_count}]'
xlg "$XLG/api/v1/environments/$ENV_ID" | jq '{id, name, variables: [.variables[].key]}'

# 在环境下单独增删改租户
TENANT=$(xlg -X POST "$XLG/api/v1/environments/$ENV_ID/tenants" --data-binary @- <<'EOF'
{
  "name": "gm-tenant",
  "weight": 2,
  "api_key": "gm-api-key",
  "encryption_mode": "full",
  "tenant_id": "t-gm-01",
  "key_version": "v1",
  "tenant_key_base64": "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
  "suite": "suite/gm-sm4-hmac-sm3",
  "binding_alg": "sm2-sign",
  "binding_kid": "kid-gm-01"
}
EOF
)
TENANT_ID=$(echo "$TENANT" | jq -r .id)

# 为环境租户生成非对称请求绑定密钥对（私钥自动存入该租户，响应只返回公钥 PEM）
xlg -X POST "$XLG/api/v1/environments/$ENV_ID/tenants/$TENANT_ID/binding-keypair" \
  --data-binary '{"alg":"sm2-sign","kid":"kid-gm-01"}' \
  | jq '{binding_kid, binding_alg, public_key_pem}'

# 无状态生成非对称绑定密钥对（供未保存的草稿请求内联租户使用，返回公私钥 PEM）
xlg -X POST "$XLG/api/v1/crypto/binding-keypair" \
  --data-binary '{"alg":"ed25519","kid":"draft-kid"}' \
  | jq '{binding_kid, binding_alg, public_key_pem}'

# 导入 / 导出环境（兼容 Hoppscotch Environment JSON 与原生格式）
xlg "$XLG/api/v1/environments/export" | jq 'length'
```

### 集合与请求（Collections & Requests）

集合（`collections`）支持通过 `parent_id` 组织无限层级目录树，可配置目录级默认协议、可继承的 HTTP Auth / Headers / Variables；请求（`requests`）归属于集合，每条请求可独立选择 `protocol`（`http` 或 `mazbri`）。

```bash
# 创建集合
COLL=$(xlg -X POST "$XLG/api/v1/collections" --data-binary @- <<'EOF'
{
  "name": "gateway-smoke",
  "default_protocol": "http",
  "description": "HTTP 与 Mazbri 双协议回归集合"
}
EOF
)
COLL_ID=$(echo "$COLL" | jq -r .id)

# 在集合内创建一条通用 HTTP 请求（支持 <<base_url>> 与 {{seq}} 动态变量）
HTTP_REQ=$(xlg -X POST "$XLG/api/v1/requests" --data-binary @- <<EOF
{
  "collection_id": "$COLL_ID",
  "name": "http-echo",
  "protocol": "http",
  "method": "POST",
  "endpoint": "<<base_url>>/ok",
  "params": [{"key": "from", "value": "curl", "active": true}],
  "headers": [{"key": "X-Trace", "value": "req-{{seq}}", "active": true}],
  "body": {
    "content_type": "application/json",
    "body": "{\"seq\":\"{{seq}}\"}",
    "form": []
  },
  "auth": {"auth_type": "bearer", "auth_active": true, "token": "<<api_token>>"},
  "assertions": {"expect_transport": "ok", "checks": []},
  "weight": 2
}
EOF
)
HTTP_REQ_ID=$(echo "$HTTP_REQ" | jq -r .id)

# 在同一集合内创建一条 Mazbri 网关协议请求（引用环境租户池或使用内联租户）
MZ_REQ=$(xlg -X POST "$XLG/api/v1/requests" --data-binary @- <<EOF
{
  "collection_id": "$COLL_ID",
  "name": "mazbri-order-query",
  "protocol": "mazbri",
  "method": "POST",
  "endpoint": "/orders/query",
  "params": [],
  "headers": [],
  "body": {
    "content_type": "application/json",
    "body": "{\"order_no\":\"ORD-{{seq}}\"}",
    "form": []
  },
  "mazbri": {
    "product_id": "P-ORDER-01",
    "tenant_source": "environment",
    "tenant_name": "gm-tenant"
  },
  "assertions": {"expect_transport": "ok", "checks": []},
  "weight": 1
}
EOF
)
MZ_REQ_ID=$(echo "$MZ_REQ" | jq -r .id)

# 查看完整集合目录树、导入 / 导出（兼容 Hoppscotch REST Collection JSON 与原生格式）
xlg "$XLG/api/v1/collections" | jq '[.[] | {id, name, requests: [.requests[].name]}]'
xlg "$XLG/api/v1/collections/export" | jq 'length'
```

### 单次交互调试（Probe / Send）

`POST /api/v1/probe` 对应工作台里的 **`Send`** 按钮：可按 `request_id` 发送已保存请求，也可直接传未保存草稿 `request`，结合可选 `environment_id` 发起 **1 次真实请求**。
- `http` 模式返回状态码、耗时（`latency_us`）、字节数、响应头、格式化 JSON、逐项断言结果与实际发出的请求预览。
- `mazbri` 模式额外返回 `mazbri_wire`（Token 换取耗时与摘要、5 个 `x-mazbri-*` 绑定签名头、内层明文 `GatewayRequest`、外层密文 `TransportEnvelope` 与解密结果）。

```bash
xlg -X POST "$XLG/api/v1/probe" --data-binary @- <<EOF | jq '{http_status, transport_ok, passed_assertions, latency_us}'
{
  "request_id": "$HTTP_REQ_ID",
  "environment_id": "$ENV_ID"
}
EOF
```

### 压测预设（Load Profiles）

压测预设（`load-profiles`）将流量模型与具体接口解耦，可复用于任意单接口、未保存草稿或整个集合。

```bash
PROFILE=$(xlg -X POST "$XLG/api/v1/load-profiles" --data-binary @- <<'EOF'
{
  "name": "nightly-once",
  "model": "open_loop",
  "qps": 20,
  "concurrency": 4,
  "requests": 50,
  "warmup_secs": 0,
  "ramp_secs": 0,
  "timeout_secs": 5,
  "connect_timeout_secs": 5,
  "token_refresh_skew_secs": 30,
  "include_request_id": true,
  "processes": 1,
  "qps_mode": "total",
  "requests_mode": "total",
  "loop_enabled": false,
  "loop_interval_secs": 2,
  "expected_reasons": []
}
EOF
)
PROFILE_ID=$(echo "$PROFILE" | jq -r .id)

LOOP_PROFILE=$(xlg -X POST "$XLG/api/v1/load-profiles" --data-binary @- <<'EOF'
{
  "name": "nightly-loop",
  "model": "open_loop",
  "qps": 10,
  "concurrency": 2,
  "requests": 20,
  "warmup_secs": 0,
  "ramp_secs": 0,
  "timeout_secs": 5,
  "connect_timeout_secs": 5,
  "token_refresh_skew_secs": 30,
  "include_request_id": true,
  "processes": 1,
  "qps_mode": "total",
  "requests_mode": "total",
  "loop_enabled": true,
  "rounds": 100,
  "loop_interval_secs": 2,
  "expected_reasons": []
}
EOF
)
LOOP_PROFILE_ID=$(echo "$LOOP_PROFILE" | jq -r .id)

xlg "$XLG/api/v1/load-profiles" | jq '[.[] | {id, name, model, qps, requests, loop_enabled}]'
```

### 启动任务（Runs）

`POST /api/v1/runs` 支持三种压测目标来源（三选一）：
1. `request_id`：压已保存的单条请求；
2. `collection_id`（可选 `selected_requests: [{request_id, weight}]` 勾选子集并覆盖权重）：对集合（含子目录）做多接口加权混合压测；
3. `adhoc_request` / `adhoc_requests`：不保存集合，直接压临时请求对象。

同时可指定 `environment_id`、可选预设 `load_profile`（名称或 ID）或 `load_profile_id`，以及本次运行的覆盖字段（只作用于本次 `snapshot`，不会修改已保存的预设）。`requests` 与 `duration_secs` 互斥，设置其中一个会清掉另一个。

可选 `assert`：`p50_ms` / `p95_ms` / `p99_ms` / `max_ms`（必须 > 0）、`max_error_rate`（0–1）、`min_qps_ratio`（> 0，且需设置 `qps`）。比较采用闭区间（等于阈值算通过）。

`?wait=true&timeout=N`：请求会等到任务离开 `running`/`stopping` 再返回 201；`timeout` 默认 60 秒、最大 600、最小 1。超时返回 **409**，`error.code` 为 `wait_timeout`，同时带上当前 `run`。

```bash
# 对单条已保存请求按预设启动一次压测，覆盖 QPS / 请求数并附带 SLA 断言，wait 等待结束
RUN=$(xlg -X POST "$XLG/api/v1/runs?wait=true&timeout=60" --data-binary @- <<EOF
{
  "request_id": "$HTTP_REQ_ID",
  "environment_id": "$ENV_ID",
  "load_profile": "nightly-once",
  "mode": "once",
  "qps": 15,
  "concurrency": 4,
  "requests": 30,
  "processes": 1,
  "assert": {
    "p99_ms": 5000,
    "max_error_rate": 0.05,
    "min_qps_ratio": 0.5
  }
}
EOF
)
echo "$RUN" | jq '{id, title, status, current_qps, verdict}'
RUN_ID=$(echo "$RUN" | jq -r .id)

# 对整个集合启动循环加权压测（通过 selected_requests 勾选请求并指定权重）
LOOP=$(xlg -X POST "$XLG/api/v1/runs" --data-binary @- <<EOF
{
  "collection_id": "$COLL_ID",
  "selected_requests": [
    {"request_id": "$HTTP_REQ_ID", "weight": 3},
    {"request_id": "$MZ_REQ_ID", "weight": 1}
  ],
  "environment_id": "$ENV_ID",
  "load_profile_id": "$LOOP_PROFILE_ID",
  "mode": "loop",
  "qps": 8,
  "rounds": 50,
  "loop_interval_secs": 2,
  "assert": {"max_error_rate": 0.2, "p99_ms": 5000}
}
EOF
)
LOOP_ID=$(echo "$LOOP" | jq -r .id)
echo "$LOOP" | jq '{id, status, mode, current_qps}'
```

同时已有任务在跑时，API **不会**因此返回 409。若触发全局 `max_qps` / `max_concurrency` 上限则 400 `invalid_request`。资源不存在 404；非法断言或互斥字段同时给出则 400。

### 运行中调整流量

只对 `running` 的任务有效：

```bash
xlg -X PATCH "$XLG/api/v1/runs/$LOOP_ID/traffic" --data-binary '{"qps":20,"concurrency":4}' \
  | jq '{id, status, current_qps, current_concurrency}'
```

`qps` 与 `concurrency` 都是可选的，只传要改的字段。超过全局上限会 400。

### 停止、续跑、删除

```bash
# 停止（对 running / stopping）；停止后再续跑
xlg -X POST "$XLG/api/v1/runs/$LOOP_ID/stop" | jq '{id, status}'
# 等到 status 变为 stopped（或 finished / failed）
xlg "$XLG/api/v1/runs/$LOOP_ID" | jq .status

# 续跑：仅 stopped / interrupted 可 resume
xlg -X POST "$XLG/api/v1/runs/$LOOP_ID/resume" | jq '{id, status}'

# 再停一次，然后删除。运行中或 stopping 的任务不能删（409），需先停止。
xlg -X POST "$XLG/api/v1/runs/$LOOP_ID/stop" | jq '{id, status}'
# 等待结束后：
xlg -X DELETE "$XLG/api/v1/runs/$LOOP_ID"    # 204，同时删除任务目录
```

`GET /api/v1/runs` 可加 `?status=running`（或 `finished`、`stopped` 等）过滤。`GET /api/v1/runs/{id}` 返回详情，含脱敏后的 `snapshot` 和 `verdict`。

### 统计、时序、时间线、窗口、事件、样本

```bash
xlg "$XLG/api/v1/runs/$RUN_ID/stats" | jq '{status, samples, success, failure, p50_us, p95_us, p99_us, max_us, current_qps, in_flight}'

xlg "$XLG/api/v1/runs/$RUN_ID/timeseries" | jq '{count: (.points | length), last: .points[-1]}'
# points[]: unix_sec、samples、success、failure、bytes 等

xlg "$XLG/api/v1/runs/$RUN_ID/timeline" | jq '.events[] | {ts, event, payload}'
# 例如 start / window / traffic / stop / end

xlg "$XLG/api/v1/runs/$RUN_ID/windows" | jq '.windows[] | {window_index, samples, success, failure, p99_us}'

# 样本：默认最多 100 条，limit 最大 1000。可按 product_id、success、from_ms、to_ms 过滤；
# 用 after_end_unix_ms、after_worker、after_seq 做 keyset 翻页（响应里的 next 原样带回）。
xlg "$XLG/api/v1/runs/$RUN_ID/samples?limit=20&success=true" | jq '{count: (.items | length), next}'

# 实时事件：SSE，事件名为 stats，约每秒一条。curl -N 关闭缓冲；Ctrl-C 结束。
# curl -NS -H @"$HDR" "$XLG/api/v1/runs/$LOOP_ID/events"
```

百分位在接口里以微秒（`*_us`）给出；断言字段则是毫秒（`*_ms`）。

### 判定（verdict）与退出码

`GET /api/v1/runs/{id}/verdict` 的 HTTP 状态是为了让 `curl -f` 直接当脚本退出码用（有意不用 202：`curl -f` 把 2xx 当成成功）：

| 结果 | HTTP | curl -f 退出码 | 含义 |
| --- | --- | --- | --- |
| `pass` 或 `none` | 200 | 0 | 通过，或该任务没有设置断言 |
| `fail` | 422 | 22 | 至少一项阈值未满足（`failed[]` 列出 check / wanted / actual） |
| `pending` | 409 | 22 | 任务仍在 `running`/`stopping`，已给出目前的 actual |

响应体始终是 `Verdict`：`result`、`assert`、`actual`（含 `samples`、`error_rate`、`p99_ms`、`achieved_qps`、`qps_ratio` 等）、`failed`。任务详情和 `report.json` 里也有同样的 `verdict`。

```bash
# 已结束且通过：-f 成功，退出 0
xlg "$XLG/api/v1/runs/$RUN_ID/verdict" | jq '{result, actual, failed}'

# 在脚本里区分 fail / pending（不要只用 -f，因为二者都是非 0）
code=$(curl -sS -o /tmp/verdict.json -w '%{http_code}' -H @"$HDR" \
  "$XLG/api/v1/runs/$RUN_ID/verdict")
jq '{result, failed}' /tmp/verdict.json
case "$code" in
  200) echo "pass 或 none" ;;
  422) echo "断言失败"; exit 2 ;;
  409) echo "仍在运行"; exit 3 ;;
  *)   echo "意外状态 $code"; exit 1 ;;
esac
```

### 下载报告

```bash
# JSON 正文（与 GET .../report 相同，另带 Content-Disposition）
xlg "$XLG/api/v1/runs/$RUN_ID/report" | jq '{id, status, counters, percentiles, verdict}'

xlg -o "run-$RUN_ID-report.json" "$XLG/api/v1/runs/$RUN_ID/report.json"
# HTML 是完整独立页面，嵌入了同一份脱敏 JSON（含 verdict）
curl -fsS -H @"$HDR" -o "run-$RUN_ID-report.html" "$XLG/api/v1/runs/$RUN_ID/report.html"
```

报告、任务快照和默认集合/环境导出会自动脱敏所有敏感凭据（`api_key`、`binding_key`、`tenant_key_base64`、`client_key`、环境变量 secret、HTTP auth 令牌/密码等）。

### 错误格式与状态码

除判定接口（正文为 `Verdict`）和部分 204 删除外，错误响应统一为：

```json
{"error":{"code":"<code>","message":"<说明>"}}
```

`wait=true` 超时时是同一结构，并多一个 `run` 字段。

| HTTP | code | 典型情况 |
| --- | --- | --- |
| 400 | `invalid_request` | 参数非法、断言越界、互斥字段、超过全局 QPS/并发上限 |
| 400 | `auth_disabled` | `--no-auth` 下调用登录或改密 |
| 401 | `unauthorized` | 未带令牌、口令错误、改密/重置后仍用旧口令 |
| 403 | `password_change_required` | 首次口令尚未更改时访问业务接口 |
| 404 | `not_found` | 集合、请求、环境、租户、压测预设、任务不存在 |
| 409 | `conflict` | 资源冲突（例如删除运行中的任务、尝试删除内置 Global 环境） |
| 409 | `wait_timeout` | `POST /runs?wait=true` 在 timeout 内未结束（附带 `run`） |
| 409 | （verdict 正文） | 判定结果为 `pending`（正文是 Verdict，不是 Error） |
| 422 | （verdict 正文） | 判定结果为 `fail`（正文是 Verdict，不是 Error） |
| 500 | `internal` | 未预期的服务端错误 |

### 端到端自动化脚本示例

下面的脚本：对已保存请求启动一个循环压测任务、轮询统计、在线把 QPS 调高、停止、下载报告并检查判定。把前面的「变量」「鉴权」以及 `$HTTP_REQ_ID`、`$ENV_ID` 准备好之后即可运行。

```bash
#!/usr/bin/env bash
# 依赖：curl、jq。令牌已写入 $TOKEN_FILE 并执行过 write_hdr。
set -euo pipefail
: "${XLG:?set XLG}" "${HDR:?run write_hdr first}" "${HTTP_REQ_ID:?set HTTP_REQ_ID}"
xlg() { curl -fsS -H @"$HDR" "$@"; }

echo "== 启动循环任务"
LOOP=$(xlg -X POST "$XLG/api/v1/runs" --data-binary @- <<EOF
{
  "request_id": "$HTTP_REQ_ID",
  "environment_id": "${ENV_ID:-}",
  "load_profile": "nightly-loop",
  "mode": "loop",
  "qps": 8,
  "concurrency": 2,
  "rounds": 50,
  "loop_interval_secs": 2,
  "assert": {"p99_ms": 5000, "max_error_rate": 0.2}
}
EOF
)
LOOP_ID=$(echo "$LOOP" | jq -r .id)
echo "$LOOP" | jq '{id, status, current_qps}'

echo "== 监控约 5 秒"
for i in 1 2 3 4 5; do
  xlg "$XLG/api/v1/runs/$LOOP_ID/stats" \
    | jq -c '{samples, success, failure, p99_us, current_qps, status}'
  sleep 1
done

echo "== 把 QPS 调到 20"
xlg -X PATCH "$XLG/api/v1/runs/$LOOP_ID/traffic" --data-binary '{"qps":20}' \
  | jq '{id, current_qps, current_concurrency, status}'

sleep 2
echo "== 停止并等待结束"
xlg -X POST "$XLG/api/v1/runs/$LOOP_ID/stop" | jq '{id, status}'
for i in $(seq 1 30); do
  ST=$(xlg "$XLG/api/v1/runs/$LOOP_ID" | jq -r .status)
  echo "  status=$ST"
  case "$ST" in
    stopped|finished|failed|interrupted) break ;;
  esac
  sleep 1
done

echo "== 下载报告"
xlg -o "run-$LOOP_ID-report.json" "$XLG/api/v1/runs/$LOOP_ID/report.json"
jq '{id, status, counters, percentiles, verdict}' "run-$LOOP_ID-report.json"

echo "== 判定"
code=$(curl -sS -o /tmp/verdict.json -w '%{http_code}' -H @"$HDR" \
  "$XLG/api/v1/runs/$LOOP_ID/verdict")
jq '{result, failed, actual}' /tmp/verdict.json
case "$code" in
  200) echo "verdict 通过或未设置断言"; exit 0 ;;
  422) echo "verdict 未通过"; exit 2 ;;
  409) echo "仍在运行（不应发生，已停止）"; exit 3 ;;
  *)   echo "意外 HTTP $code"; exit 1 ;;
esac
```

运行中的循环任务不要对 `POST /runs?wait=true` 设过短的 timeout 却期望 201：到点会 409 `wait_timeout`，任务本身继续跑，用返回体里的 `run.id` 再去 stop。

### 在 Web 工作台里调试与压测

登录（或 `--no-auth`）之后可以在 Web 工作台完成全部交互调试与压测操作：

1. **配置环境与租户池**：在工作台顶栏或右侧栏「环境（Environments）」新建环境，配置键值变量（如 `base_url`）、默认 `gateway_base_url`、TLS 证书与 Mazbri 加权租户池。
2. **多标签接口调试（`发送`）**：在主工作台新建标签页或导入 cURL，切换协议（`HTTP 通用` / `Mazbri 网关`），配置参数、Headers、CodeMirror 请求体、鉴权与响应断言，点击 **`发送`** 即可查看格式化响应、断言结果，以及 Mazbri 专属的「信封与签名追踪」。
3. **组织集合与预设**：点击「保存」将请求存入右侧「集合（Collections）」目录树（支持 JSON 导入导出）；在左侧导航「负载预设」管理可复用的开环/闭环流量模型与 SLA 阈值。
4. **发起单接口或集合加权压测（`压测`）**：在任意请求栏点击「压测」或在集合节点上点击「集合压测」，勾选参与压测的接口、调整各自权重（`weight`）、选择预设或临时覆盖 QPS/并发/多进程参数并一键启动。
5. **实时监控与报告**：跳转至 `/runs/:id` 查看 ECharts 实时吞吐曲线、延迟分位卡片、在线热调 QPS/并发、停止/续跑、游标分页样本明细表，并一键下载脱敏 JSON 或独立离线 HTML 报告。

`gateway_base_url` 必须是**容器能够访问**的地址：IP、公网 URL，或 DNS 可解析的域名加端口。不要填写 `http://127.0.0.1:8888` 来指向宿主机上的网关（那是容器自己的回环）。指向宿主机上的监听时，使用宿主机在该桥接网络上的地址，或该服务对容器可见的地址。

页面路由：`/`（多标签调试工作台）、`/runs`（压测任务列表）、`/runs/:id`（实时监控与报告详情）、`/profiles`（负载预设管理）、`/docs`（内置可视化 OpenAPI 接口规范）、`/settings`（`Light` / `Black` 跟随系统外观与安全设置）、`/login`、`/password`。

## 接入 mazbri 网关（加密与签名）

请求协议选 **`Mazbri`** 时，x-loadgen 按 mazbri 官方客户端合同发请求，加解密逻辑完全来自仓库内的 `x-loadgen-libs`，报文格式与网关一致：

1. 换 token：`POST {gateway_base_url}/auth/token`，body 为 `{"api_key":"...","grant_type":"client_credentials"}`。token 按进程、按租户缓存，过期前（`expires_in` 减去刷新提前量）单飞刷新；网关返回 401 时作废缓存并重新换取。
2. 调用：`POST {gateway_base_url}/api/v1/gateway`，带 `Authorization: Bearer <token>`。请求体为 `GatewayRequest`（`product_id`、`path`、`method`、`query`、`headers`、`body`），请求里的 `<<var>>` / `{{var}}` 与压测动态变量 `{{seq}}`、`{{$timestamp}}`、`{{$randomUUID}}` 在发送前自动渲染。
3. 请求绑定（可选）：对 `METHOD\npath\ntimestamp\nnonce\nbody` 签名，附带 `x-mazbri-timestamp`、`x-mazbri-nonce`、`x-mazbri-signature`、`x-mazbri-signature-input`、`x-mazbri-key-id`。
4. 加密：`none` 发明文；`request` 只密封请求，响应按明文解析；`full` 请求和响应都加密，响应解密并校验 MAC 后才参与统计。

耗时、状态统计和业务校验（`assertions.expect_transport`、`assertions.checks`）都基于解密后的响应。

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

Mazbri 请求既可以引用当前激活环境中的加权租户池（支持多租户混合压测，也可指定单一租户名），也可以在当前请求标签页直接使用「内联独立租户（Inline Tenant）」快速联调。每个租户可填写：API Key、加密模式、租户 ID、密钥版本、租户密钥（标准 Base64）、套件或自定义 `data_alg`/`mac_alg`、信封签名要求、允许不安全算法；请求绑定填写算法、Key ID，以及 HMAC 密钥或 PKCS#8 PEM 私钥。

非对称绑定无需手工准备私钥文件：填写算法与 Key ID 后点击「一键生成非对称密钥对」，环境租户会把私钥安全保存在本机 SQLite 并展示公钥 PEM，复制公钥到网关管理端（或联调时的 `mockgw`）登记该 `kid` 即可。

API Key、租户密钥、HMAC 密钥和私钥在编辑表单里默认遮盖，可点眼睛图标切换显示；环境列表、任务详情、报告和默认导出始终脱敏，公钥不属于敏感字段。

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

### 通用 HTTP 协议

请求协议选 **`HTTP`** 时直接按标准 HTTP 客户端访问目标 URL（支持绝对 URL，或结合环境 `gateway_base_url` 拼接相对路径），原生支持任意 Method、多同名 Query / Header 启停行、`application/json` / `application/x-www-form-urlencoded` / `multipart/form-data` / `text/plain` / `application/xml` 请求体，以及 `Inherit` / `None` / `Bearer` / `Basic` / `API Key`（Header 或 Query）鉴权。

## Mock 网关

`x-loadgen-mockgw` 按同一份合同实现 `/auth/token` 与 `/api/v1/gateway`：校验 token、请求绑定签名、打开请求信封，并把请求原样回显（`data.echo`），`full` 模式下加密响应。它用于自动化测试和本地端到端联调：

```bash
cargo run -p x-loadgen-mockgw -- --example > /tmp/mockgw.json   # 示例配置
cargo run -p x-loadgen-mockgw -- --listen 0.0.0.0:18080 --config /tmp/mockgw.json
```

配置字段与页面上的租户字段同名，另有 `binding_verify_key`（HMAC 密钥或签名公钥 PEM）和故障注入 `fault`（`none` / `tamper_response` / `plaintext_response`）。运行中可以用 `PUT /__mock/tenants` 替换租户配置（例如登记页面生成的公钥），`GET /__mock/stats` 查看已签发 token、已验签与已解封的计数，`GET /__mock/health` 做存活检查。

容器内的 x-loadgen 访问宿主机上的 mock 网关时，`gateway_base_url` 填宿主机的局域网 IP，或 `${STACK}-x-loadgen-net` 网络的网关地址，例如 `http://<宿主机IP>:18080`。
