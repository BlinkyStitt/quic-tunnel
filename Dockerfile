FROM rust:1.74-bookworm as builder

WORKDIR /app

COPY . .

RUN --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    \
    cargo install --path .

FROM debian:bookworm-slim

ENTRYPOINT [ "/quic-tunnel" ]

COPY --link --from=builder /usr/local/cargo/bin/quic-tunnel /quic-tunnel

RUN /quic-tunnel --help
