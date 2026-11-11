#!/usr/bin/env bash
# 启动（数据保留）：准备目录与属主、校验镜像、up -d 并等待健康，打印访问地址与口令文件路径
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$SCRIPT_DIR/env.sh"
svc_start "$@"
