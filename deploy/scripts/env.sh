#!/usr/bin/env bash
# 组件环境 + 钩子（被同目录脚本 source）
COMPONENT=x-loadgen
PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$PROJECT_ROOT/lib/common.sh"
lib_load

IMAGE="${X_LOADGEN_IMAGE}:${X_LOADGEN_VERSION}"

# 本机没有镜像时先 docker pull（Docker Hub 发版镜像）；拉不到再提示本地构建或离线导入
require_image() {
  docker image inspect "$IMAGE" >/dev/null 2>&1 && return 0
  log "本机没有镜像 $IMAGE，尝试 docker pull"
  docker pull "$IMAGE" >/dev/null 2>&1 && { log "已拉取 $IMAGE"; return 0; }
  die "镜像 $IMAGE 不存在且无法拉取。
  联网主机：确认 config/.env 里 X_LOADGEN_IMAGE=dex2all/x-loadgen 且版本已发布
  本地构建：在仓库根 docker build -t x-loadgen:${X_LOADGEN_VERSION} .，并设 X_LOADGEN_IMAGE=x-loadgen
  离线主机：gunzip -c x-loadgen-${X_LOADGEN_VERSION}.tar.gz | docker load，X_LOADGEN_IMAGE 与导入的镜像名一致"
}

# 探测本机内网 IP：ip route src -> hostname -I 第一个 -> 127.0.0.1
detect_ip() {
  local ip=""
  if command -v ip >/dev/null 2>&1; then
    ip="$(ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<NF;i++) if($i=="src"){print $(i+1); exit}}')"
  fi
  [ -n "$ip" ] || ip="$(hostname -I 2>/dev/null | awk '{print $1}')"
  printf '%s\n' "${ip:-127.0.0.1}"
}

# 访问地址：只开本机时为 127.0.0.1；对外发布时用 X_LOADGEN_ACCESS_HOST 或自动探测
access_urls() {
  local host
  case "$X_LOADGEN_BIND" in
    127.0.0.1|localhost) printf 'http://127.0.0.1:%s/（只开本机）\n' "$X_LOADGEN_PORT"; return 0;;
    0.0.0.0) host="${X_LOADGEN_ACCESS_HOST:-$(detect_ip)}";;
    *) host="${X_LOADGEN_ACCESS_HOST:-$X_LOADGEN_BIND}";;
  esac
  printf 'http://%s:%s/\n' "$host" "$X_LOADGEN_PORT"
  [ "$host" = 127.0.0.1 ] || printf 'http://127.0.0.1:%s/（本机）\n' "$X_LOADGEN_PORT"
}

# 鉴权提示：只打印口令文件路径，不打印内容
auth_hint() {
  local pass="$PROJECT_ROOT/data/admin.passwd"
  if [ "${X_LOADGEN_NO_AUTH:-0}" = 1 ] || [ "${X_LOADGEN_NO_AUTH:-0}" = true ]; then
    log "鉴权: 已关闭（X_LOADGEN_NO_AUTH=${X_LOADGEN_NO_AUTH}），任何能访问端口的人都可操作"
  elif [ -f "$pass" ]; then
    log "鉴权: 首次登录口令文件 $pass（容器内 /data/admin.passwd，登录改密后自动删除）"
  elif [ -f "$PROJECT_ROOT/data/loadgen.sqlite" ]; then
    log "鉴权: 已改密（口令文件已删除）；忘记口令执行 scripts/reset-passwd.sh"
  else
    log "鉴权: 口令文件尚未生成（服务首次启动时写入 data/admin.passwd）"
  fi
}

# 属主守卫：已有 data/ logs/ 属主必须是执行者；不一致时默认退出，FIX_OWNER=1 时用一次性容器改正
ownership_guard() {
  local d u bad=()
  for d in data logs; do
    [ -e "$d" ] || continue
    u="$(stat -c %u "$d")"
    [ "$u" = "$RUN_UID" ] || bad+=("$d(uid=$u)")
  done
  [ ${#bad[@]} -eq 0 ] && return 0
  if [ "${FIX_OWNER:-0}" = 1 ]; then
    helper_run chown -R "$RUN_UID:$RUN_GID" /proj/data /proj/logs
    log "已将 data/ logs/ 属主改为 $RUN_UID:$RUN_GID"
  else
    die "${bad[*]} 属主与执行者 uid=$RUN_UID 不符（容器以执行者身份运行）。
  请用原来的用户执行，或确认后执行: FIX_OWNER=1 $PROJECT_ROOT/scripts/start.sh"
  fi
}

hook_prepare() {
  require_image
  ownership_guard
  # data/logs 是 logs/ 的嵌套挂载点，预先建好，避免 dockerd 以 root 创建
  mkdir -p data/logs
}

hook_post_start() {
  local i
  for ((i = 0; i < 20; i++)); do
    if [ -f data/admin.passwd ] || [ -f data/loadgen.sqlite ]; then break; fi
    sleep 0.5
  done
  log "镜像: $IMAGE（容器内监听 0.0.0.0:8757，发布 ${X_LOADGEN_BIND}:${X_LOADGEN_PORT}）"
  local u; while IFS= read -r u; do log "访问地址: $u"; done < <(access_urls)
  auth_hint
}

svc_status() {
  local st
  st="$(docker inspect -f '{{.State.Status}}{{if .State.Health}} ({{.State.Health.Status}}){{end}}' "$CONTAINER_NAME" 2>/dev/null)" || st="absent"
  printf '%-14s %-26s %s\n' "$COMPONENT" "$CONTAINER_NAME" "$st"
  printf '%-14s %-26s %s\n' "  image" "$IMAGE" "$(docker image inspect -f '{{.Id}}' "$IMAGE" 2>/dev/null | cut -c8-19 || echo absent)"
  local u; while IFS= read -r u; do printf '%-14s %s\n' "  url" "$u"; done < <(access_urls)
  printf '%-14s %s\n' "  data" "$PROJECT_ROOT/data"
  auth_hint
}

# 运行中任务的 pending-*.bin 数量（停机未排空时出现，续跑时导回）
pending_count() { find "$PROJECT_ROOT/data/runs" -name 'pending-*.bin' 2>/dev/null | wc -l; }

# 备份：SQLite（中央库 + 各任务库 + WAL）、检查点与 pending-*.bin 必须处于一致状态。
# 运行中则先 docker stop（serve 停投、排空、写检查点），打包 data/（不含 logs/），再按原状态启动。
hook_backup() {
  local was=0 out="backups/x-loadgen_${1:?缺少时间戳}.tar.gz"
  if container_running; then
    was=1
    warn "为保证 SQLite 与检查点一致，备份期间短暂停止服务；运行中的任务会写检查点，之后可在页面续跑"
    svc_stop
  fi
  log "pending-*.bin: $(pending_count) 个（随数据一起备份）"
  ( umask 077; tar -C data --exclude=./logs -czf "$out" . ) || die "打包 data/ 失败"
  log "备份: $out（含 SQLite 与签名私钥，按密钥材料保管）"
  if [ $was = 1 ]; then svc_start; fi
}

# 恢复：确认 -> 校验备份 -> 下线 -> 当前 data/ 移到 backups/pre-restore_<ts>/ -> 解包 -> 启动
hook_restore() {
  local f="$1"; shift
  f="$(cd -- "$(dirname -- "$f")" && pwd)/$(basename -- "$f")"
  case "$f" in *.tar.gz) ;; *) die "仅支持 backup.sh 生成的 x-loadgen_<时间>.tar.gz";; esac
  tar -tzf "$f" >/dev/null || die "备份文件损坏: $f"
  confirm "将用 $(basename "$f") 覆盖当前 data/（当前数据移到 backups/pre-restore_<时间>/）。" "$@"
  prepare_dirs
  svc_down
  local keep; keep="backups/pre-restore_$(date +%Y%m%d_%H%M%S)"
  mkdir -m 700 "$keep"
  if [ -n "$(ls -A data)" ]; then
    find data -mindepth 1 -maxdepth 1 ! -name logs -exec mv -t "$keep" {} +
  fi
  ( umask 077; tar -C data -xzf "$f" )
  log "当前数据已移到 $keep/"
  svc_start
  log "已从 $(basename "$f") 恢复（pending-*.bin: $(pending_count) 个）"
}

# 进程日志：logs/ 下按日滚动的 serve.<日期> 与 run-<id>-wNN.<日期>
svc_logs() {
  local n=200 follow=() file="" a
  for a in "$@"; do
    case "$a" in
      -f) follow=(-F);;
      ''|*[!0-9]*) file="$a";;
      *) n="$a";;
    esac
  done
  if [ -z "$file" ]; then
    file="$(find "$PROJECT_ROOT/logs" -maxdepth 1 -type f -name 'serve.*' -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -n1 | cut -d' ' -f2-)"
    [ -n "$file" ] || die "logs/ 下还没有 serve 日志"
  else
    [ -f "$file" ] || file="$PROJECT_ROOT/logs/$file"
    [ -f "$file" ] || die "日志不存在: $file"
  fi
  log "日志: $file"
  tail -n "$n" "${follow[@]}" "$file"
}

# 重置管理口令：在容器内重新生成 data/admin.passwd（只打印路径）
svc_reset_passwd() {
  require_running
  dc exec -T x-loadgen x-loadgen reset-passwd --data-dir /data >/dev/null
  auth_hint
}
