#!/usr/bin/env bash
# From a fresh clone to the project open in Xcode, in one command.
# Needs Xcode, Rust (https://rustup.rs) and Homebrew.
set -euo pipefail

cd "$(dirname "$0")/.."

command -v xcodegen >/dev/null || brew install xcodegen
scripts/build-core.sh
[ -f Config/Local.xcconfig ] || cp Config/Local.example.xcconfig Config/Local.xcconfig
xcodegen generate
open Venti.xcodeproj
