#!/usr/bin/env bash
# Builds the Rust core for the browser: a WebAssembly module, the JavaScript
# that loads it and the TypeScript declarations of everything it exports.
# The result goes to apps/web/src/lib/core/pkg, which is not committed.
#
# Needs Rust from rustup, and `npm ci` to have run in apps/web. Run it again
# whenever crates/core, crates/core-wasm or anything below them changes.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"

out="apps/web/src/lib/core/pkg"

if [ ! -d apps/web/node_modules/binaryen ]; then
  echo "Run 'npm ci' in apps/web first: the last step here uses a tool it installs." >&2
  exit 1
fi

# The tool that writes the JavaScript must be the very version of the
# wasm-bindgen library the core was compiled with. Cargo.lock names it.
# The tool that binds the core must be of exactly the version of the
# wasm-bindgen crate in Cargo.lock. It is built and kept inside the
# checkout, under target/tools: each checkout has the version its lock file
# asks for, and a machine that runs only what was built under the project's
# folder (the owner's Mac) runs it.
version="$(cargo pkgid --locked -p wasm-bindgen)"
version="${version##*@}"
bindgen="$root/target/tools/bin/wasm-bindgen"
if [ "$("$bindgen" --version 2>/dev/null)" != "wasm-bindgen $version" ]; then
  CARGO_TARGET_DIR="$root/target/tools-build" \
    cargo install wasm-bindgen-cli --version "$version" --locked --root "$root/target/tools"
fi

rustup target add wasm32-unknown-unknown
cargo build --locked -p venti-core-wasm --target wasm32-unknown-unknown --profile wasm

rm -rf "$out"
# The page says where the module is; no second, guessed location is built in.
"$bindgen" --target web --omit-default-module-path --out-dir "$out" \
  target/wasm32-unknown-unknown/wasm/venti_core_wasm.wasm

# About half the module is the core's interface for the phones and what only
# that reaches. Take it out.
node apps/web/scripts/strip-core.mjs "$out/venti_core_wasm_bg.wasm"
ls -l "$out"
