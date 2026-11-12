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

命令行本身不能发起压测。所有自动化（建目标、建模板、启动任务、调流量、取报告、判定）都通过 HTTP API 完成，推荐用 `curl` + `jq`。本节覆盖全部接口，例子均可直接复制。完整机器可读清单见无需认证的 [OpenAPI 文档](#openapi-文档)。

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

完整 OpenAPI 3.0 文档（公开，无需登录）：

```text
GET $XLG/api/v1/openapi.json
```

可导入 Postman、Insomnia，或用浏览器打开。路径与运行中的路由一一对应。

### 目标（Targets）

协议为 `http`（普通 HTTP）或 `mazbri`。列表不返回密钥；详情（`GET /targets/{id}`）包含密钥明文，请妥善保管输出。

```bash
# 创建一个普通 HTTP 目标（租户只能是 encryption_mode=none，不能填加密或绑定字段）
TARGET=$(xlg -X POST "$XLG/api/v1/targets" --data-binary @- <<EOF | jq .
{
  "name": "http-demo",
  "protocol": "http",
  "gateway_base_url": "$GW",
  "insecure_skip_verify": true,
  "note": "curl 手册演示",
  "tenants": [
    {"name": "plain", "weight": 1, "api_key": "demo-key", "encryption_mode": "none"}
  ],
  "products": [
    {
      "name": "probe",
      "product_id": "P001",
      "path": "/ok",
      "method": "GET",
      "weight": 1
    }
  ]
}
EOF
)
echo "$TARGET" | jq '{id, name, protocol}'
TARGET_ID=$(echo "$TARGET" | jq -r .id)

xlg "$XLG/api/v1/targets" | jq '[.[] | {id, name, protocol, gateway_base_url}]'
xlg "$XLG/api/v1/targets/$TARGET_ID" | jq '{id, name, tenants: [.tenants[].name], products: [.products[].name]}'

# 更新（PUT）。省略 tenants / products 时保留原有子资源；省略 client_cert、client_key、ca_cert 等密钥字段会被清空，修改时要带上原值或有意覆盖
xlg -X PUT "$XLG/api/v1/targets/$TARGET_ID" --data-binary @- <<EOF | jq '{id, note}'
{"name":"http-demo","protocol":"http","gateway_base_url":"$GW","insecure_skip_verify":true,"note":"已更新"}
EOF

# 删除（若仍有模板引用会 409 conflict）
# xlg -X DELETE "$XLG/api/v1/targets/$TARGET_ID"    # 204 无响应体
```

mazbri 目标在创建时一并给出租户的协议与密码学字段（`encryption_mode`、`suite` 或 `data_alg`/`mac_alg`、`tenant_id`、`tenant_key_base64`、`key_version`、`binding_*` 等），详见下文「租户」和后面的「接入 mazbri 网关」。API Key 在普通 HTTP 下以 `X-API-KEY` 头发送。

### 租户（Tenants）

挂在目标下，可单独增删改。普通 HTTP 目标只接受 `encryption_mode=none` 的明文租户。

```bash
# 列表 / 创建
xlg "$XLG/api/v1/targets/$TARGET_ID/tenants" | jq '[.[] | {id, name, weight, encryption_mode}]'

TENANT=$(xlg -X POST "$XLG/api/v1/targets/$TARGET_ID/tenants" --data-binary @- <<'EOF'
{"name":"plain-2","weight":1,"api_key":"demo-key-2","encryption_mode":"none"}
EOF
)
TENANT_ID=$(echo "$TENANT" | jq -r .id)
echo "$TENANT" | jq '{id, name, weight}'

xlg "$XLG/api/v1/targets/$TARGET_ID/tenants/$TENANT_ID" | jq '{id, name, encryption_mode}'

xlg -X PUT "$XLG/api/v1/targets/$TARGET_ID/tenants/$TENANT_ID" --data-binary @- <<'EOF' | jq '{id, weight}'
{"name":"plain-2","weight":2,"api_key":"demo-key-2","encryption_mode":"none"}
EOF

# 删除：204
xlg -X DELETE "$XLG/api/v1/targets/$TARGET_ID/tenants/$TENANT_ID"
```

为 mazbri 租户生成请求绑定密钥对（私钥存进租户，响应 **只返回公钥**）：

```bash
# 目标协议必须是 mazbri；alg 例如 sm2-sign
xlg -X POST "$XLG/api/v1/targets/$MAZBRI_TARGET_ID/tenants/$MAZBRI_TENANT_ID/binding-keypair" \
  --data-binary '{"alg":"sm2-sign","kid":"kid-curl-demo"}' \
  | jq '{binding_kid, binding_alg, public_key_pem}'
```

租户上与协议 / 密码学相关的字段包括：`api_key`、`encryption_mode`（`none` / `request` / `response` / `full`）、`tenant_id`、`tenant_key_base64`、`key_version`、`suite`、`data_alg`、`mac_alg`、`require_signature`、`allow_insecure`、`binding_kid`、`binding_alg`、`binding_key`、`binding_public_key`。生成密钥对后 `binding_key` 由服务保存，详情接口会返回，列表和报告会脱敏。

### 产品（Products）

```bash
xlg "$XLG/api/v1/targets/$TARGET_ID/products" | jq '[.[] | {id, name, product_id, path, method, weight}]'

PRODUCT=$(xlg -X POST "$XLG/api/v1/targets/$TARGET_ID/products" --data-binary @- <<'EOF'
{
  "name": "echo",
  "product_id": "P-ECHO",
  "path": "/ok",
  "method": "POST",
  "weight": 1,
  "body_json": {"seq": "{{seq}}"}
}
EOF
)
PRODUCT_ID=$(echo "$PRODUCT" | jq -r .id)

xlg "$XLG/api/v1/targets/$TARGET_ID/products/$PRODUCT_ID" | jq '{id, name, method, path}'

xlg -X PUT "$XLG/api/v1/targets/$TARGET_ID/products/$PRODUCT_ID" --data-binary @- <<'EOF' | jq '{id, weight}'
{"name":"echo","product_id":"P-ECHO","path":"/ok","method":"POST","weight":2,"body_json":{"seq":"{{seq}}"}}
EOF

# 删除：204
xlg -X DELETE "$XLG/api/v1/targets/$TARGET_ID/products/$PRODUCT_ID"
```

可选字段：`qps_limit`、`timeout_secs`、`query_json`、`headers_json`、`body_json`、`expect_transport`、`expect_checks_json`。

### 模板（Templates）

模板保存一次压测的全部参数，启动任务必须引用一份已保存的模板（按 id 或名称），并可以在启动时覆盖部分字段。

```bash
TEMPLATE=$(xlg -X POST "$XLG/api/v1/templates" --data-binary @- <<EOF
{
  "name": "nightly-once",
  "target_id": "$TARGET_ID",
  "model": "open_loop",
  "qps": 20,
  "concurrency": 4,
  "requests": 50,
  "warmup_secs": 0,
  "ramp_secs": 0,
  "timeout_secs": 5,
  "connect_timeout_secs": 5,
  "processes": 1,
  "qps_mode": "total",
  "requests_mode": "total",
  "loop_enabled": false
}
EOF
)
TEMPLATE_ID=$(echo "$TEMPLATE" | jq -r .id)
echo "$TEMPLATE" | jq '{id, name, qps, requests, loop_enabled}'

xlg "$XLG/api/v1/templates" | jq '[.[] | {id, name, qps, requests, duration_secs, loop_enabled}]'
xlg "$XLG/api/v1/templates/$TEMPLATE_ID" | jq '{id, name, target_id, qps, concurrency, processes}'

# 再存一份循环模板，供 loop 模式使用
LOOP_TPL=$(xlg -X POST "$XLG/api/v1/templates" --data-binary @- <<EOF
{
  "name": "nightly-loop",
  "target_id": "$TARGET_ID",
  "model": "open_loop",
  "qps": 10,
  "concurrency": 2,
  "requests": 20,
  "timeout_secs": 5,
  "processes": 1,
  "loop_enabled": true,
  "rounds": 100,
  "loop_interval_secs": 2
}
EOF
)
LOOP_TEMPLATE_ID=$(echo "$LOOP_TPL" | jq -r .id)

# 更新 / 删除
# xlg -X PUT "$XLG/api/v1/templates/$TEMPLATE_ID" --data-binary @file.json
# xlg -X DELETE "$XLG/api/v1/templates/$TEMPLATE_ID"    # 204；被运行中任务占用时可能 409
```

常用字段：`model`（`open_loop` / `closed_loop`）、`qps`、`concurrency`、`requests` 与 `duration_secs` 二选一、`warmup_secs`、`ramp_secs`、`timeout_secs`、`connect_timeout_secs`、`token_refresh_skew_secs`、`include_request_id`、`processes`（≥1，大于 1 时按进程拆分）、`qps_mode` / `requests_mode`（`total` 或 `per_process`）、`loop_enabled`、`rounds`（空表示循环不限轮次）、`loop_interval_secs`、`product_ids`、`weight_overrides`、`expected_reasons`、`detail_retain_days`、`detail_retain_rows`、`note`。

### 启动任务（Runs）

`POST /api/v1/runs` 必须指定已保存的模板：`template`（id 或名称，优先）或 `template_id`。同一次请求里可以带覆盖字段，只作用于这次运行的内存副本，**已保存的模板不会被改掉**。`requests` 与 `duration_secs` 互斥，设置其中一个会清掉另一个。`rounds` 与 `clear_rounds` 不能同时给；`clear_rounds: true` 用于把循环改为不限轮次。

`mode` 为 `once` 或 `loop`（可省略，默认跟随模板的 `loop_enabled`）。`once` 跑完 `requests` 或 `duration_secs` 即结束；`loop` 按 `rounds` 与 `loop_interval_secs` 重复窗口。`processes`、`concurrency`、`qps` 控制进程数、并发与目标速率；`qps_mode=total` 表示所有进程合计，`per_process` 表示每个进程各自那么多。

可选 `assert`：`p50_ms` / `p95_ms` / `p99_ms` / `max_ms`（必须 > 0）、`max_error_rate`（0–1）、`min_qps_ratio`（> 0，且创建时模板或覆盖里要有 `qps`）。比较采用闭区间（等于阈值算通过）。达成 QPS = 样本数 ÷ 墙钟耗时（`started_at` 到 `ended_at` 或当前时间）。断言冻结在任务快照里。

`?wait=true&timeout=N`：请求会等到任务离开 `running`/`stopping` 再返回 201；`timeout` 默认 60 秒、最大 600、最小 1。超时返回 **409**，`error.code` 为 `wait_timeout`，同时带上当前 `run`。

```bash
# 按模板 id 启动一次任务（once），不覆盖参数
xlg -X POST "$XLG/api/v1/runs" --data-binary @- <<EOF | jq '{id, status, template_id, current_qps, mode}'
{"template_id":"$TEMPLATE_ID","mode":"once"}
EOF

# 按模板名称启动，并覆盖 QPS / 请求数 / 进程，附带断言；wait 等到结束
RUN=$(xlg -X POST "$XLG/api/v1/runs?wait=true&timeout=60" --data-binary @- <<'EOF'
{
  "template": "nightly-once",
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
echo "$RUN" | jq '{id, status, current_qps, verdict}'
RUN_ID=$(echo "$RUN" | jq -r .id)

# 确认已保存的模板没被改掉
xlg "$XLG/api/v1/templates/$TEMPLATE_ID" | jq '{qps, requests}'
# 覆盖只出现在这次任务的 snapshot 里
xlg "$XLG/api/v1/runs/$RUN_ID" | jq '{status, snapshot_qps: .snapshot.template.qps, snapshot_requests: .snapshot.template.requests, verdict}'

# 按名称启动循环任务（不要 wait，或接受 409 wait_timeout）
LOOP=$(xlg -X POST "$XLG/api/v1/runs" --data-binary @- <<'EOF'
{
  "template": "nightly-loop",
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

同时已有任务在跑时，API **不会**因此返回 409（页面会先确认）。若触发全局 `max_qps` / `max_concurrency` 上限则 400 `invalid_request`。模板不存在 404；非法断言或两个互斥字段同时给出 400。

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
# points[]: unix_sec 或 started_at、samples、success、failure、bytes 等

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
# HTML 是完整页面，嵌入了同一份 JSON（含 verdict）
curl -fsS -H @"$HDR" -o "run-$RUN_ID-report.html" "$XLG/api/v1/runs/$RUN_ID/report.html"
```

报告和快照会脱敏密钥（`api_key`、`binding_key`、`tenant_key_base64`、`client_key` 等）。

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
| 404 | `not_found` | 目标、租户、产品、模板、任务不存在 |
| 409 | `conflict` | 资源冲突（例如删除仍被引用的目标、删除运行中的任务） |
| 409 | `wait_timeout` | `POST /runs?wait=true` 在 timeout 内未结束（附带 `run`） |
| 409 | （verdict 正文） | 判定结果为 `pending`（正文是 Verdict，不是 Error） |
| 422 | （verdict 正文） | 判定结果为 `fail`（正文是 Verdict，不是 Error） |
| 500 | `internal` | 未预期的服务端错误 |

### 端到端自动化脚本示例

下面的脚本：按名称启动一个循环任务、轮询统计、把 QPS 调高、停止、下载报告、检查判定。把前面的「变量」「鉴权」和已有的 `nightly-loop` 模板准备好之后即可运行。

```bash
#!/usr/bin/env bash
# 依赖：curl、jq。令牌已写入 $TOKEN_FILE 并执行过 write_hdr。
set -euo pipefail
: "${XLG:?set XLG}" "${HDR:?run write_hdr first}"
xlg() { curl -fsS -H @"$HDR" "$@"; }

echo "== 启动循环任务"
LOOP=$(xlg -X POST "$XLG/api/v1/runs" --data-binary @- <<'EOF'
{
  "template": "nightly-loop",
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

### 在页面里配置并启动任务

登录（或 `--no-auth`）之后也可以用页面完成同样的事情：

1. 打开「目标」，新建目标。选择协议（mazbri 网关 / 普通 HTTP），填写名称和 `gateway_base_url`，再配置租户与产品。mazbri 的加密与签名字段见下一节。
2. 打开「模板」，新建模板并绑定目标。单次任务填写 `requests` 或 `duration_secs`；循环任务打开 loop，填写 `rounds` 与 `loop_interval_secs`。
3. 打开「任务」，选择模板和模式（`once` / `loop`），可选填写断言后启动。若已有正在运行的任务，页面会先确认再启动；API 不会只因为另有任务在跑就返回 409。
4. 任务详情页可看实时统计、判定、调整流量、停止或续跑。报告页查看脱敏摘要和判定，并可下载 JSON / HTML。

`gateway_base_url` 必须是**容器能够访问**的地址：IP、公网 URL，或 DNS 可解析的域名加端口。不要填写 `http://127.0.0.1:8888` 来指向宿主机上的网关（那是容器自己的回环）。指向宿主机上的监听时，使用宿主机在该桥接网络上的地址，或该服务对容器可见的地址。

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
