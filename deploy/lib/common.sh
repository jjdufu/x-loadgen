#!/usr/bin/env bash
# 公共函数库：被 scripts/env.sh source。
# 约定：调用前已设置 COMPONENT 与 PROJECT_ROOT（工程目录，含 config/ scripts/ lib/ common/）。
# 全部路径都由脚本自身位置推导，整个工程目录可改名、移动到任意位置，在任意 cwd 下执行。
set -euo pipefail

: "${COMPONENT:?COMPONENT 未设置}"
: "${PROJECT_ROOT:?PROJECT_ROOT 未设置}"

log()  { printf '[%s] %s\n' "$COMPONENT" "$*"; }
warn() { printf '[%s] 警告: %s\n' "$COMPONENT" "$*" >&2; }
die()  { printf '[%s] 错误: %s\n' "$COMPONENT" "$*" >&2; exit 1; }

lib_load() {
  local env_file="$PROJECT_ROOT/config/.env"
  if [ ! -f "$env_file" ]; then
    [ -f "$PROJECT_ROOT/config/.env.example" ] || die "缺少 config/.env 与 config/.env.example"
    ( umask 077; cp "$PROJECT_ROOT/config/.env.example" "$env_file" )
    log "已从 config/.env.example 生成 config/.env（按需修改后重新执行）"
  fi
  set -a
  # shellcheck source=../config/.env.example
  . "$env_file"
  set +a
  export STACK="${STACK:-mazbri}"
  # 容器以“执行者”的 uid:gid 运行（只取数字，不取用户名）。
  # sudo 执行时沿用发起 sudo 的普通用户，避免产生 root 属主文件。
  export RUN_UID="${SUDO_UID:-$(id -u)}"
  export RUN_GID="${SUDO_GID:-$(id -g)}"
  export CONTAINER_NAME="${STACK}-${COMPONENT}"
  # 一次性辅助容器默认用本工程镜像（离线主机只需 docker load 这一个镜像）
  HELPER_IMAGE="${HELPER_IMAGE:-x-loadgen:${X_LOADGEN_VERSION:?X_LOADGEN_VERSION 未设置}}"
  COMPOSE_ARGS=(--project-directory "$PROJECT_ROOT/config" -f "$PROJECT_ROOT/config/compose.yml")
}

dc() { docker compose "${COMPOSE_ARGS[@]}" "$@"; }
has_hook() { declare -F "$1" >/dev/null; }

# 以 root 跑一次性辅助容器（处理非执行者属主的文件），工程目录挂到 /proj
helper_run() {
  docker run --rm --entrypoint "" -u 0 -v "$PROJECT_ROOT:/proj" "$HELPER_IMAGE" "$@"
}

container_running() {
  [ "$(docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME" 2>/dev/null || true)" = "true" ]
}
require_running() { container_running || die "容器 $CONTAINER_NAME 未运行，请先 scripts/start.sh"; }

# 危险操作确认：参数含 --yes/-y 直接通过；终端下要求输入 yes；否则拒绝
confirm() {
  local prompt="$1" a ans; shift
  for a in "$@"; do case "$a" in --yes|-y) return 0;; esac; done
  if [ -t 0 ]; then
    read -r -p "$prompt 输入 yes 确认: " ans || ans=""
    [ "$ans" = yes ] && return 0
  fi
  die "已取消（非交互执行请追加 --yes）"
}

prepare_dirs() {
  cd "$PROJECT_ROOT"
  mkdir -p data logs backups
  chmod 700 data logs backups 2>/dev/null || true
  chmod 600 config/.env 2>/dev/null || true
}

# 启动：准备目录与属主 -> compose up --wait（依赖 healthcheck）-> 启动后钩子
svc_start() {
  prepare_dirs
  if has_hook hook_prepare; then hook_prepare; fi
  log "启动中（等待 healthcheck）..."
  local t0=$SECONDS
  if ! dc up -d --remove-orphans --wait --wait-timeout "${WAIT_TIMEOUT:-120}"; then
    dc ps -a || true
    dc logs --tail 80 || true
    die "启动失败或健康检查超时"
  fi
  if has_hook hook_post_start; then hook_post_start; fi
  log "就绪（$((SECONDS - t0))s），容器: $CONTAINER_NAME"
  dc ps
}

svc_stop() { log "停止容器（保留容器与数据）"; dc stop; }
svc_down() { log "下线容器与网络（保留数据）"; dc down --remove-orphans; }

# 删除：下线 + 清空工程内 data/ logs/ backups/
# 普通用户删不掉的（其它 uid 属主的文件）用一次性容器删除。
svc_remove() {
  confirm "将删除容器、网络以及 data/ logs/ backups/（含 SQLite、签名私钥、全部备份）。" "$@"
  log "下线容器并清空 data/ logs/ backups/ ..."
  dc down --remove-orphans --volumes
  cd "$PROJECT_ROOT"
  local d
  for d in data logs backups; do
    [ -e "$d" ] || continue
    if ! rm -rf "$d" 2>/dev/null; then
      helper_run rm -rf "/proj/$d"
    fi
    [ -e "$d" ] && die "无法删除 $d/"
    log "已删除 $d/"
  done
  log "已清空"
}

svc_status() {
  local st
  st="$(docker inspect -f '{{.State.Status}}{{if .State.Health}} ({{.State.Health.Status}}){{end}}' "$CONTAINER_NAME" 2>/dev/null)" || st="absent"
  printf '%-14s %-26s %s\n' "$COMPONENT" "$CONTAINER_NAME" "$st"
}

svc_backup() {
  has_hook hook_backup || { log "该组件不做备份，跳过"; return 0; }
  prepare_dirs
  local ts; ts="$(date +%Y%m%d_%H%M%S)"
  hook_backup "$ts"
  local keep="${BACKUP_RETENTION_DAYS:-7}"
  find "$PROJECT_ROOT/backups" -mindepth 1 -maxdepth 1 -mtime +"$keep" -exec rm -rf {} + 2>/dev/null || true
  log "备份完成，目录: backups/"; ls -la "$PROJECT_ROOT/backups"
}

svc_restore() {
  has_hook hook_restore || die "该组件不支持恢复"
  [ $# -ge 1 ] || die "用法: restore <backups/ 下的备份名> [--yes]"
  local name="$1"; shift
  local f="$name"; [ -e "$f" ] || f="$PROJECT_ROOT/backups/$name"
  [ -e "$f" ] || die "备份不存在: $name"
  hook_restore "$f" "$@"
}
