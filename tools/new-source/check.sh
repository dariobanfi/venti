#!/bin/sh
# Holds one adapter crate to what every adapter meets.
#
#   tools/new-source/check.sh <name>
#
# For crates/sources-<name>: the formatting, the lints, the crate's own
# tests (its golden files and, for a station network, the contract of
# venti_sources::testing::check_station_adapter), and the server's tests of
# its list of adapters and of the region packs, which hold every adapter
# and every pack of the build together.
#
# It asks no provider and needs no network beyond what cargo needs.
set -eu

if [ $# -ne 1 ]; then
    sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi
name=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
if [ ! -d "crates/sources-$name" ]; then
    echo "there is no crates/sources-$name" >&2
    exit 1
fi

set -x
cargo fmt -p "venti-sources-$name" -- --check
cargo clippy -p "venti-sources-$name" --all-targets -- -D warnings
cargo test -p "venti-sources-$name"
cargo test -p venti-server --lib -- adapters:: plan::
set +x

snake=$(printf '%s' "$name" | tr '-' '_')
if ! grep -q "venti_sources_$snake::" crates/server/src/adapters.rs; then
    echo "note: crates/server/src/adapters.rs does not name venti_sources_$snake, so the server" >&2
    echo "      does not have this adapter yet: docs/adding-a-source.md, step 1." >&2
fi
