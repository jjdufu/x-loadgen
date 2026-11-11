#!/usr/bin/env bash
# 重置管理口令（重新生成 data/admin.passwd，只打印路径）
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_reset_passwd "$@"
