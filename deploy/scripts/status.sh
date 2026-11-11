#!/usr/bin/env bash
# 查看状态、访问地址与口令文件路径
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_status "$@"
