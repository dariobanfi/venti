# The venti-server image.
#
# Build it from the repository root, so that the whole Rust workspace is in
# the build context:
#
#     docker build -f deploy/Dockerfile -t venti-server .
#
# `deploy/docker-compose.yml` does exactly that.

# --- Build -------------------------------------------------------------------
# The compiler version must be the one pinned in rust-toolchain.toml; CI
# checks that. The file itself stays out of the build: it also asks for
# rustfmt and clippy, which the build would then download and never use.
FROM rust:1.97.0-slim-bookworm AS build
WORKDIR /build

# Cargo needs the manifest of every workspace member to resolve the
# workspace, also when only the server is built.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY tools/uniffi-bindgen-swift tools/uniffi-bindgen-swift
# The development mode compiles the first region pack in.
COPY regions regions

# The cache mounts keep downloaded crates and compiled dependencies between
# builds on the same machine, so an update rebuilds only what changed.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked -p venti-server \
    && cp target/release/venti-server /usr/local/bin/venti-server

# --- Run ---------------------------------------------------------------------
FROM debian:bookworm-slim

# ca-certificates: the server checks the data providers' TLS certificates
# against the system's list. curl: the container's health check.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin venti \
    && install -d -o venti -g venti /app /var/lib/venti/archive /var/lib/venti/imagery

COPY --from=build /usr/local/bin/venti-server /usr/local/bin/venti-server
COPY regions /app/regions

USER venti
WORKDIR /app

# Defaults that fit the container; `docker-compose.yml` sets the rest.
ENV VENTI_LISTEN=0.0.0.0:8080 \
    VENTI_REGIONS=/app/regions/alps-de-at/region.toml \
    VENTI_ARCHIVE_DIR=/var/lib/venti/archive \
    VENTI_IMAGERY_DIR=/var/lib/venti/imagery

EXPOSE 8080
VOLUME /var/lib/venti/archive /var/lib/venti/imagery

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl --fail --silent --show-error --output /dev/null http://127.0.0.1:8080/v1/health

ENTRYPOINT ["venti-server"]
CMD ["serve"]
