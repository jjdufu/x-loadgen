# syntax=docker/dockerfile:1.7

# 前端：在 web/ 执行 pnpm install --frozen-lockfile && pnpm build。
FROM node:20-bookworm AS frontend
WORKDIR /src/web
RUN corepack enable && corepack prepare pnpm@10.34.6 --activate
COPY x-loadgen-bin/x-loadgen/web/package.json x-loadgen-bin/x-loadgen/web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY x-loadgen-bin/x-loadgen/web/ ./
RUN pnpm build

# Rust：将源码与 mazbri-libs 放到保持相对 path 的位置，并嵌入前端 dist。
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
    | sh -s -- -y --default-toolchain 1.99.1 --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /src/x-loadgen
COPY . /src/x-loadgen
COPY --from=libs / /src/mazbri-max/mazbri-libs
COPY --from=frontend /src/web/dist /src/x-loadgen/x-loadgen-bin/x-loadgen/web/dist
RUN cargo build --release -p x-loadgen \
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
