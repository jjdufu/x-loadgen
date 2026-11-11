#!/usr/bin/env bash
# 下线容器与网络（保留数据）
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_down "$@"
