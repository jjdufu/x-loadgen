#!/usr/bin/env bash
# 停止容器（保留容器与数据）；serve 收到 SIGTERM 后停投、排空、写检查点
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_stop "$@"
