# syntax=docker/dockerfile:1.7

# 前端：在 web/ 执行 pnpm install --frozen-lockfile && pnpm build。
FROM node:20-bookworm AS frontend
WORKDIR /src/web
RUN corepack enable && corepack prepare pnpm@10.34.6 --activate
COPY x-loadgen-bin/x-loadgen/web/package.json x-loadgen-bin/x-loadgen/web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY x-loadgen-bin/x-loadgen/web/ ./
RUN pnpm build

# Rust：workspace 自包含（x-loadgen-libs 为成员 crate），嵌入前端 dist。
FROM debian:trixie AS builder
ARG CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=${CARGO_REGISTRIES_CRATES_IO_PROTOCOL}
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain none --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /src/x-loadgen
# 工具链唯一来源是 rust-toolchain.toml；单独一层，改源码不重装工具链。
COPY rust-toolchain.toml ./
RUN rustup toolchain install && rustc -V
COPY . /src/x-loadgen
COPY --from=frontend /src/web/dist /src/x-loadgen/x-loadgen-bin/x-loadgen/web/dist
RUN cargo build --release --locked -p x-loadgen \
    && test -x /src/x-loadgen/target/release/x-loadgen

# 运行镜像：bundled rusqlite，不安装 libsqlite3；不含编译器与源码树。
FROM debian:trixie-slim AS runtime
ENV DEBIAN_FRONTEND=noninteractive
ENV TZ=Asia/Shanghai
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 mazbri \
    && useradd --uid 10001 --gid 10001 --home-dir /data --shell /usr/sbin/nologin mazbri \
    && mkdir -p /data \
    && chown 10001:10001 /data
COPY --from=builder /src/x-loadgen/target/release/x-loadgen /usr/local/bin/x-loadgen
USER 10001:10001
VOLUME ["/data"]
EXPOSE 8757
ENTRYPOINT ["tini", "--", "x-loadgen"]
CMD ["serve", "--listen", "0.0.0.0:8757", "--data-dir", "/data"]
