# syntax=docker/dockerfile:1.6

# ---- build stage: glibc, native toolchain ----
FROM rust:1-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      pkg-config cmake clang perl make \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# Copy the workspace. .dockerignore keeps target/ and other junk out.
COPY . .

ENV CARGO_NET_RETRY=10 \
    CARGO_TERM_COLOR=never

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p hermes-cli \
 && cp target/release/hermes /out-hermes

# ---- runtime stage: debian-slim, glibc ----
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 hermes

COPY --from=builder /out-hermes /usr/local/bin/hermes

# All lebi-AI state lives under $HOME/.lebi-ai/. Point HOME at the
# bind-mount target so the existing `dirs::home_dir()` calls resolve there.
ENV HOME=/data

WORKDIR /data
USER hermes

ENTRYPOINT ["/usr/local/bin/hermes"]
CMD ["wechat", "run"]
