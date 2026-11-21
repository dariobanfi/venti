#!/usr/bin/env bash
# Runs the web app's checks in a container, for a machine where `npm ci`
# cannot install the pinned packages (the owner's Mac: see AGENTS.md beside
# this folder). Needs Docker.
#
#     scripts/checks-in-a-container.sh        licences, formatting, types, unit tests
#     scripts/checks-in-a-container.sh e2e    the same, then the browser tests
#
# The image is the deployment's `site` stage (deploy/web.Dockerfile): it
# builds the core as WebAssembly from this checkout, installs the packages
# and builds the site. Nothing is written into the checkout but
# apps/web/screenshots and apps/web/test-results, which git ignores.
#
# Extra arguments after `e2e` go to Playwright: `e2e --project=phone-light`,
# `e2e e2e/thermal-map.spec.ts`.
#
# The browser tests need a Venti development server and the made-up day in
# front of it, which this script starts on this machine and stops at the
# end, on ports 8290 and 8291 (VENTI_CHECKS_PORT and the one after it). The
# server is target/debug/venti-server as it was built last: build it again
# after a change to the server.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"

e2e=false
if [ "${1:-}" = "e2e" ]; then
  e2e=true
  shift
fi

started=$SECONDS

# The site, built from the checkout as the deployment builds it.
docker build -f deploy/web.Dockerfile --target site -t venti-web-site:local .

# What the tests read from elsewhere in the repository and the image does
# not hold: the server's golden files and the sample flight, the label fonts
# and the elevation tiles of the region the apps bundle.
read_only=(
  -v "$root/crates:/build/crates:ro"
  -v "$root/apps/ios/Venti/Resources:/build/apps/ios/Venti/Resources:ro"
)

docker run --rm "${read_only[@]}" venti-web-site:local \
  sh -c 'npm run licences && npm run format:check && npm run check && npm test'

if ! $e2e; then
  echo "Checks passed in $(((SECONDS - started) / 60)) min $(((SECONDS - started) % 60)) s."
  exit 0
fi

# --- The browser tests ---------------------------------------------------------
# The browser and its system packages, and the typeface the screenshots are
# taken in (e2e/fonts.conf), as the Web workflow installs them. They are a
# stage of their own that depends only on Playwright's version, so that
# Docker keeps it and the browser is downloaded once, not on every run.
playwright="$(sed -n 's/.*"@playwright\/test": "\([^"]*\)".*/\1/p' apps/web/package.json)"
docker build -t venti-web-e2e:local -f - apps/web/e2e <<EOF
FROM node:22-bookworm-slim AS browser
RUN npx -y playwright@$playwright install --with-deps chromium \
    && apt-get update -qq \
    && apt-get install -y --no-install-recommends fonts-inter \
    && rm -rf /var/lib/apt/lists/*
COPY fonts.conf /root/.config/fontconfig/fonts.conf
RUN fc-cache -f && fc-match system-ui | grep Inter

FROM browser
COPY --from=venti-web-site:local /build /build
WORKDIR /build/apps/web
EOF

port="${VENTI_CHECKS_PORT:-8290}"
made_up_port=$((port + 1))
logs="$(mktemp -d)"
servers=()
stop() {
  # Written so that an empty list passes `set -u` in the Mac's bash 3.2.
  for pid in ${servers[@]+"${servers[@]}"}; do kill "$pid" 2>/dev/null || true; done
}
trap stop EXIT

for each in "$port" "$made_up_port"; do
  if curl -s -o /dev/null "http://127.0.0.1:$each/"; then
    echo "Port $each is taken. Name another with VENTI_CHECKS_PORT." >&2
    exit 1
  fi
done

if [ ! -x target/debug/venti-server ]; then
  echo "Build the server first: cargo build --locked -p venti-server" >&2
  exit 1
fi
# As the Web workflow's tests start it: the recorded data, no map regions.
target/debug/venti-server dev --listen "127.0.0.1:$port" >"$logs/venti-server.log" 2>&1 &
servers+=($!)
python3 tools/made-up-day/serve.py --behind "http://127.0.0.1:$port" --port "$made_up_port" \
  >"$logs/made-up-day.log" 2>&1 &
servers+=($!)

for url in "http://127.0.0.1:$port/v1/health" "http://127.0.0.1:$made_up_port/v1/thermals"; do
  for _ in $(seq 60); do
    curl -sf -o /dev/null "$url" && continue 2
    sleep 1
  done
  echo "Nothing answered at $url. Logs: $logs" >&2
  exit 1
done

# Screenshots and what failing tests leave behind land where a run on this
# machine puts them.
mkdir -p apps/web/screenshots apps/web/test-results
# The container reaches this machine's 127.0.0.1 as host.docker.internal.
docker run --rm "${read_only[@]}" \
  -v "$root/apps/web/screenshots:/build/apps/web/screenshots" \
  -v "$root/apps/web/test-results:/build/apps/web/test-results" \
  -e VENTI_DEV_SERVER="http://host.docker.internal:$port" \
  -e VENTI_MADE_UP_DAY_SERVER="http://host.docker.internal:$made_up_port" \
  venti-web-e2e:local npm run test:e2e -- "$@"

echo "Checks and browser tests passed in $(((SECONDS - started) / 60)) min $(((SECONDS - started) % 60)) s."
