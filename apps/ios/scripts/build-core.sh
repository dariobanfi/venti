#!/usr/bin/env bash
# Builds the Rust core for Apple platforms and packages it for the Swift
# package in apps/ios/Packages/VentiCore. Run on a Mac with Xcode and Rust.
#
# It builds for the iPhone, and for the simulator and the Mac of the machine
# it runs on. The Intel path is not exercised by CI.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"

package="apps/ios/Packages/VentiCore"
work="target/apple"
case "$(uname -m)" in
  arm64) targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin) ;;
  x86_64) targets=(aarch64-apple-ios x86_64-apple-ios x86_64-apple-darwin) ;;
  *) echo "unsupported Mac architecture: $(uname -m)" >&2; exit 1 ;;
esac
host="${targets[2]}"

rustup target add "${targets[@]}"
for target in "${targets[@]}"; do
  cargo build --release --locked -p venti-core --target "$target"
done

rm -rf "$work/headers" "$package/VentiCoreFFI.xcframework" "$package/Sources/VentiCore/Generated"
mkdir -p "$work/headers" "$package/Sources/VentiCore/Generated"

# The bindings are the same for every target, so they are generated from one
# library. The module map must name the module the generated Swift imports.
library="target/$host/release/libventi_core.a"
cargo run --locked -p uniffi-bindgen-swift -- \
  "$library" "$package/Sources/VentiCore/Generated" --swift-sources
cargo run --locked -p uniffi-bindgen-swift -- \
  "$library" "$work/headers" --headers \
  --modulemap --module-name venti_coreFFI --modulemap-filename module.modulemap

arguments=()
for target in "${targets[@]}"; do
  arguments+=(-library "target/$target/release/libventi_core.a" -headers "$work/headers")
done
xcodebuild -create-xcframework "${arguments[@]}" -output "$package/VentiCoreFFI.xcframework"
