# x-loadgen（Compose 工程）

x-loadgen 的独立部署工程，布局与 mazbri 中间件的单组件 Compose 工程一致：`config/compose.yml` + `config/.env` + `scripts/*.sh` + `lib/common.sh` + `common/base.yml`，运行期数据都在工程目录内的 `data/`、`logs/`、`backups/`。

本目录自包含，不依赖仓库外或其它工程的任何文件。所有路径都由脚本自身位置推导，compose 里只用相对路径：整个目录可以改名、拷到任意位置，在任意工作目录下执行 `<目录>/scripts/start.sh` 都能用。

## 目录

```text
<工程目录>/
  README.md
  .gitignore                # data/ logs/ backups/ config/.env 不入库
  .shellcheckrc             # shellcheck 按脚本所在目录解析 source
  common/base.yml           # extends 公共服务：restart unless-stopped、json-file 日志 50m×5 压缩
  lib/common.sh             # 公共函数：加载 config/.env、RUN_UID/RUN_GID、dc、start/stop/down/remove/backup/restore
  config/
    compose.yml             # 服务定义（只用相对路径与环境变量）
    .env.example            # 配置模板，入库
    .env                    # 首次执行脚本时由模板生成（600），本机修改，不入库
  scripts/
    env.sh                  # 组件环境与钩子（镜像校验、属主守卫、访问地址、备份恢复）
    start.sh stop.sh status.sh down.sh remove.sh backup.sh restore.sh logs.sh reset-passwd.sh
  data/                     # 挂到容器 /data：中央库、各任务库、检查点、pending-*.bin、admin.passwd
  logs/                     # 挂到容器 /data/logs：serve 与各 worker 按日滚动的进程日志
  backups/                  # backup.sh 的输出
```

## 命名与运行身份

- compose 工程名、容器名：`${STACK}-x-loadgen`；网络：`${STACK}-x-loadgen-net`。`STACK` 默认 `mazbri`，在 `config/.env` 修改。同一主机部署两份时改 `STACK` 与 `X_LOADGEN_PORT`。
- 容器以**执行者**的 uid:gid 运行（`RUN_UID`/`RUN_GID`，sudo 时取发起 sudo 的用户），`data/`、`logs/`、`backups/` 都归执行者，权限 700。镜像默认用户是 uid 10001；compose 用 `user:` 覆盖后，进程对数据目录的读写只取决于宿主机目录属主，`HOME` 指向 `/data`。
- 已有 `data/`、`logs/` 属主不是执行者时（例如换了用户执行），`start.sh` 直接退出并给出提示，不静默改属主。确认后用 `FIX_OWNER=1 scripts/start.sh`，由一次性容器把属主改成执行者。

## 首次使用

镜像为 `${X_LOADGEN_IMAGE}:${X_LOADGEN_VERSION}`，默认 `dex2all/x-loadgen`，即 GitHub Actions 发版推到 Docker Hub 的多架构镜像（linux/amd64 + linux/arm64）。`start.sh` 启动前检查镜像，本机没有时自动 `docker pull`。不能访问 Docker Hub 的主机，用本地构建或 `docker save` / `docker load` 导入的镜像，并把 `X_LOADGEN_IMAGE` 改成导入后的镜像名（见下文「镜像来源」）。

```bash
<工程目录>/scripts/start.sh     # 首次会从 config/.env.example 生成 config/.env
<工程目录>/scripts/status.sh
```

`start.sh` 依次：准备 `data/ logs/ backups/` 并校验属主 → 校验镜像（缺失时拉取）→ `docker compose up -d --wait`（等待 healthcheck）→ 打印访问地址与口令文件路径。输出示例：

```text
[x-loadgen] 访问地址: http://192.168.1.10:8757/
[x-loadgen] 访问地址: http://127.0.0.1:8757/（本机）
[x-loadgen] 鉴权: 首次登录口令文件 <工程目录>/data/admin.passwd（容器内 /data/admin.passwd，登录改密后自动删除）
```

访问地址里的 IP 取 `X_LOADGEN_ACCESS_HOST`；留空时自动探测本机内网 IP（`ip route get 1.1.1.1` 的 src → `hostname -I` 第一个 → `127.0.0.1`），只用于打印。

用 `admin.passwd` 里的口令登录页面，系统要求立即改密；改密后该文件被删除，重启也不会再生成。新口令至少 24 个字符（可打印 ASCII，不含空白、引号、分号、逗号和反斜杠）。忘记口令时执行 `scripts/reset-passwd.sh`，重新生成 `data/admin.passwd`，脚本只打印路径，不打印内容。

登录口令就是长期令牌，不自动过期，只在改密或 `reset-passwd` 后失效。脚本用 `Authorization: Bearer <口令>`（或 cookie `x_loadgen_token`）调用 API，不必先登录。完整的 curl 手册（全部接口、变量约定、端到端脚本、错误码）写在仓库根目录 [README.md](../README.md) 的「用 curl 调用 HTTP API」。联调可设 `X_LOADGEN_NO_AUTH=1`。

## 命令

| 脚本 | 说明 |
|---|---|
| `scripts/start.sh` | 启动（数据保留），等待 healthy，打印访问地址和 `admin.passwd` 路径 |
| `scripts/stop.sh` | 停止容器（保留容器与数据）。serve 收到 SIGTERM 后停投、排空写库、写检查点 |
| `scripts/status.sh` | 容器状态与健康、镜像、访问地址、数据目录、鉴权状态 |
| `scripts/down.sh` | 删除容器与网络，数据保留 |
| `scripts/remove.sh [--yes]` | 删除容器、网络和 `data/ logs/ backups/`。终端下要求输入 `yes`，非交互必须带 `--yes` |
| `scripts/backup.sh` | 打包 `data/`（不含日志）到 `backups/x-loadgen_<时间>.tar.gz`，见下文 |
| `scripts/restore.sh <备份名> [--yes]` | 用备份覆盖 `data/`，当前数据先移到 `backups/pre-restore_<时间>/` |
| `scripts/logs.sh [行数] [-f] [文件名]` | 查看 `logs/` 下最新的 serve 日志，或指定的 worker 日志 |
| `scripts/reset-passwd.sh` | 重置管理口令 |

## 配置（config/.env）

| 变量 | 默认 | 说明 |
|---|---|---|
| `STACK` | `mazbri` | 容器、工程、网络名前缀 |
| `TZ` | `Asia/Shanghai` | 时区 |
| `X_LOADGEN_IMAGE` | `dex2all/x-loadgen` | 镜像仓库名。本地构建或离线导入时改为 `x-loadgen` |
| `X_LOADGEN_VERSION` | `0.4.1` | 镜像标签，发版时自动更新为最新版本 |
| `X_LOADGEN_BIND` | `0.0.0.0` | 宿主机发布地址。`0.0.0.0` 局域网可访问；`127.0.0.1` 只开本机 |
| `X_LOADGEN_PORT` | `8757` | 宿主机端口，映射到容器内 8757 |
| `X_LOADGEN_ACCESS_HOST` | 空 | 打印访问地址用的主机名或 IP，空为自动探测 |
| `X_LOADGEN_NO_AUTH` | `0` | `1` 关闭登录（仅联调） |
| `X_LOADGEN_MAX_QPS` / `X_LOADGEN_MAX_CONCURRENCY` | 空 | 全局硬上限（所有运行中任务合计），空为不限 |
| `LG_CPUS` / `LG_MEM` / `LG_CPUS_RESERVE` / `LG_MEM_RESERVE` | `4` / `4G` / `0.5` / `256M` | 资源限制与预留，按压测机规格调整 |
| `LG_NOFILE` | `262144` | 文件句柄上限（高并发连接） |
| `X_LOADGEN_STOP_GRACE` | `120s` | `docker stop` 宽限，需大于请求超时加写库余量 |
| `BACKUP_RETENTION_DAYS` | `7` | `backups/` 保留天数 |

改完 `config/.env` 后执行 `scripts/start.sh`，compose 会按新配置重建容器。

## 网络

- 默认桥接，网络 `${STACK}-x-loadgen-net`。容器内固定监听 `0.0.0.0:8757`：桥接下端口映射把流量送到容器 eth0，只听回环则映射进来的连接到不了进程。对外开放范围由宿主机侧 `X_LOADGEN_BIND` 决定。
- 容器能访问宿主机能访问的 IP、公网地址和 DNS 域名（走 Docker 默认的 DNS 转发）。宿主机 `/etc/hosts` 里手写的映射不会进入容器，目标地址请写 IP 或 DNS 可解析的域名。
- 目标指向宿主机上的服务时，不要写 `127.0.0.1`（那是容器自己的回环），写宿主机的局域网 IP，或该桥接网络的网关地址（`docker network inspect ${STACK}-x-loadgen-net` 查看）。宿主机防火墙需放行容器网段。
- 切换到 host 网络：在 `config/compose.yml` 的服务下加 `network_mode: host`，删除 `ports` 段，其余不变；进程直接占用宿主机 8757。桥接可以这样无缝切到 host，反过来需要重新规划端口。

## 停止、检查点与 pending

镜像以 tini 为 PID 1，`docker stop` 发送 SIGTERM，serve 停止投递、排空写线程、写检查点，运行中的任务标为可续跑。`stop_grace_period` 由 `X_LOADGEN_STOP_GRACE` 控制（默认 120s）；超时被 SIGKILL 时，未落库的明细会留在 `data/runs/<任务>/pending-*.bin`，下次启动或续跑时导回。

## 备份与恢复

`data/` 里有中央库与各任务库（SQLite WAL）、检查点、`pending-*.bin`，以及页面生成的签名私钥和租户密钥（明文），备份文件要按密钥材料保管（权限 600）。

- `backup.sh`：为保证 SQLite、检查点和 pending 文件处于一致状态，服务在运行时会先 `stop`（等同正常停机，运行中的任务写检查点），打包 `data/`（不含 `logs/`），再按原状态启动。停机时间通常几秒；运行中的任务之后在页面续跑。服务本来没运行时直接打包。超过 `BACKUP_RETENTION_DAYS` 的旧备份顺带删除。
- `restore.sh <备份名>`：校验压缩包后要求确认，`down` → 当前 `data/` 移到 `backups/pre-restore_<时间>/` → 解包 → `start`。

## 镜像来源

- **Docker Hub（默认）**：`X_LOADGEN_IMAGE=dex2all/x-loadgen`，`X_LOADGEN_VERSION` 填已发布的版本号（见 GitHub Releases 或 Docker Hub 的 tag）。`start.sh` 发现本机没有该镜像时自动拉取，也可以手动 `docker pull dex2all/x-loadgen:<版本>`。
- **本地构建**：在仓库根执行 `docker build -t x-loadgen:<版本> .`，并在 `config/.env` 里设 `X_LOADGEN_IMAGE=x-loadgen`。
- **离线主机**：在联网主机拉取或构建后导出，再到目标主机导入，`X_LOADGEN_IMAGE` 与导入的镜像名一致：

```bash
docker pull dex2all/x-loadgen:<版本>
docker save dex2all/x-loadgen:<版本> | gzip > x-loadgen-<版本>.tar.gz
# 在目标主机
gunzip -c x-loadgen-<版本>.tar.gz | docker load
```

部署到其它主机时，把本目录整个拷过去（不带 `data/ logs/ backups/` 和 `config/.env`），准备好镜像后执行 `scripts/start.sh`。本工程独立启动，不接入 mazbri-max 的 `up.sh` 或平台编排。

## 检查

```bash
cd <工程目录> && shellcheck -x scripts/*.sh lib/common.sh
<工程目录>/scripts/status.sh
```
