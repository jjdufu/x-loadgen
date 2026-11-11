#!/usr/bin/env bash
# 从 backups/ 恢复：restore.sh <备份名> [--yes]
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_restore "$@"
