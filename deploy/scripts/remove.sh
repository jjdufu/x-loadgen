#!/usr/bin/env bash
# 下线并清空本工程 data/ logs/ backups/：remove.sh [--yes]
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_remove "$@"
