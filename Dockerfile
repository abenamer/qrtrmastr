# Quartermaster, the `qm` command, built in one stage and run from a small image in the next.
# The binary needs no database at build time: the migrations are embedded from ./migrations and
# there are no compile-time checked queries yet (SQLX_OFFLINE is set so that stays true).
# Base images are pinned by digest; Dependabot proposes the moves.
FROM rust:1.99-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS builder
WORKDIR /build
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked --bin qm

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 10001 --create-home --shell /usr/sbin/nologin qm
COPY --from=builder /build/target/release/qm /usr/local/bin/qm
USER qm
WORKDIR /work
ENTRYPOINT ["qm"]
CMD ["--help"]
