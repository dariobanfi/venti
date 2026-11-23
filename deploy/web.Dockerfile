# The web app, built from this repository into an image that holds nothing
# but the site's files and puts them where Caddy serves them from.
#
# Build it from the repository root:
#
#     docker build -f deploy/web.Dockerfile -t venti-web .
#
# `deploy/docker-compose.yml` does exactly that, and runs the image once at
# each start: it copies the site into the volume `web` and ends.

# --- The core as WebAssembly ---------------------------------------------------
# The same steps as apps/web/scripts/build-core.sh, with the compiler the
# server image is built with.
FROM rust:1.97.0-slim-bookworm AS core
WORKDIR /build
RUN rustup target add wasm32-unknown-unknown

# The tool that writes the JavaScript beside the module, in the version
# Cargo.lock names for the library: the two must match. Read from the lock
# file alone, so that this slow step is built again only when the lock
# file changes.
COPY Cargo.lock ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    version="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)" \
    && test -n "$version" \
    && cargo install wasm-bindgen-cli --version "$version" --locked

# Cargo needs the manifest of every workspace member to resolve the
# workspace, also when only the core is built.
COPY Cargo.toml ./
COPY crates crates
COPY tools/uniffi-bindgen-swift tools/uniffi-bindgen-swift
COPY regions regions
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked -p venti-core-wasm --target wasm32-unknown-unknown --profile wasm \
    && wasm-bindgen --target web --omit-default-module-path --out-dir /out/pkg \
        target/wasm32-unknown-unknown/wasm/venti_core_wasm.wasm

# --- The site --------------------------------------------------------------------
# The web app asks for Node 22.17 or newer (apps/web/package.json).
FROM node:22-bookworm-slim AS site
WORKDIR /build/apps/web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci

# What the build reads from elsewhere in the repository: the design pack,
# the shared map style, and the label fonts and the sample flight the apps
# share.
COPY design /build/design
COPY map /build/map
COPY apps/ios/Venti/Resources/MapDemo/fonts /build/apps/ios/Venti/Resources/MapDemo/fonts
COPY apps/ios/Venti/Resources/sample.igc /build/apps/ios/Venti/Resources/sample.igc
COPY docs /build/docs
COPY crates /build/crates
COPY apps/web ./
COPY --from=core /out/pkg src/lib/core/pkg
RUN node scripts/strip-core.mjs src/lib/core/pkg/venti_core_wasm_bg.wasm \
    && npm run build

# --- What runs ---------------------------------------------------------------------
# Nothing but a shell and the site. Started, it empties the folder Caddy
# serves and fills it again, so that no file of an older build is left
# beside the new ones, and ends.
FROM busybox:1.37
COPY --from=site /build/apps/web/build /site
VOLUME /srv/web
CMD ["sh", "-c", "find /srv/web -mindepth 1 -delete && cp -a /site/. /srv/web/ && echo \"web app: $(find /srv/web -type f | wc -l) files in /srv/web\""]
