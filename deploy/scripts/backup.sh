#!/usr/bin/env bash
# 备份 data/ 到 backups/（运行中会短暂停机以保证 SQLite 与检查点一致）
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_backup "$@"
