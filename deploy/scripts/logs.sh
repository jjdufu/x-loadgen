#!/usr/bin/env bash
# 查看进程日志：logs.sh [行数] [-f] [日志文件名]
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_logs "$@"
