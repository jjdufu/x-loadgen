# 独立 compose 启动 x-loadgen

默认使用桥接网络。容器内监听 `0.0.0.0:8757`，这样宿主机通过 `ports` 映射进来的连接才能到达进程。健康检查访问容器内的 `http://127.0.0.1:8757/health`，在进程听 `0.0.0.0` 时回环同样可达。

对外暴露范围由宿主机的 `ports` 绑定决定：

- 默认 `8757:8757`：宿主机所有地址的 8757 可进入容器。
- `X_LOADGEN_HOST_BIND=127.0.0.1:`：得到 `127.0.0.1:8757:8757`，只有本机可访问。

## 启动

在 box 上先按仓库根 `README.md` 构建镜像 `x-loadgen:0.1.0`。然后：

```bash
cd /workspace/rust-proj/x-loadgen/deploy
cp env.example .env   # 按需修改
mkdir -p data && sudo chown 10001:10001 data
docker compose --env-file .env up -d
# 浏览器打开 http://127.0.0.1:8757
```

宿主机数据目录必须对 uid 10001 可写。全部持久状态在 `/data`（对应 `deploy/data/`）：`admin.passwd`、中央库、工人库、live 快照、`pending-*.bin`、日志。容器重建后这些数据仍在。

镜像已使用 tini 作为 PID 1，不必再写 `init: true`。`docker stop` 向 tini 发送 SIGTERM，再转给 `serve`；`stop_grace_period` 为 60 秒。超时后可能留下 `pending-*.bin`，下次启动或续跑再导入。

## 环境变量

见 `env.example`。`X_LOADGEN_NO_AUTH=1` 时无需登录即可调 API 和打开页面。为 0 时口令文件位于 `/data/admin.passwd`，首次登录后必须改密。重置口令：

```bash
docker compose exec x-loadgen x-loadgen reset-passwd --data-dir /data
```

覆盖容器命令即可执行 `reset-passwd`、`version`、`health`。

## 目标地址

在页面里配置的 `gateway_base_url` 必须是容器能够访问的地址：IP、公网 URL，或 DNS 可解析的域名加端口。不要填写 `http://127.0.0.1:8888` 来指向宿主机上的服务（那是容器自己的回环）。指向宿主机上的监听时，使用宿主机在该桥接网络上的地址，或该服务对容器可见的地址。

mazbri 协议的目标在页面上完成全部接入配置（加密模式、租户密钥、套件或自定义算法、请求绑定）。非对称绑定点击「生成签名密钥对」后，私钥保存在 `/data` 下的中央库里，页面只给出公钥；因此 `deploy/data/` 需要按密钥材料的级别保管和备份。

## 用 mock 网关做端到端检查

`x-loadgen-mockgw` 不在镜像里，在宿主机上运行：

```bash
cd /workspace/rust-proj/x-loadgen
cargo run --release -p x-loadgen-mockgw -- --listen 0.0.0.0:18080 --config /path/to/mockgw.json
```

容器里的目标地址填 `http://<宿主机桥接地址>:18080`（默认 docker0 为 `172.17.0.1`；compose 自建网络时取该网络的网关地址）。页面生成公钥后，用 `PUT /__mock/tenants` 把公钥写入对应租户的 `binding_verify_key`，再启动任务；`GET /__mock/stats` 可核对 token、验签和解封计数。宿主机防火墙需放行容器网段到 18080 的访问。

## 切换到 host 网络

桥接是默认值。若要切到 host：在服务上增加 `network_mode: host`，同时去掉 `ports`（host 网络下 ports 映射无意义）。其余项（镜像、环境变量、数据卷、健康检查、`stop_grace_period`、监听 `0.0.0.0:8757`）保持不变。切换后进程直接占用宿主机的 8757。

## 镜像分发

镜像只在 box 本地构建。拷到其它主机：

```bash
docker save x-loadgen:0.1.0 | gzip > x-loadgen-0.1.0.tar.gz
# 在目标主机
gunzip -c x-loadgen-0.1.0.tar.gz | docker load
```

不要在 o50 上用本仓库 Dockerfile 构建。本 compose 独立启动，不写入 mazbri-max 的 `up.sh` 或平台编排。
