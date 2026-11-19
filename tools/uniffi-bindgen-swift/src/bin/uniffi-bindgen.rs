//! Generates the Kotlin bindings for `venti-core` from its compiled library.
//!
//! ```text
//! cargo run -p uniffi-bindgen-swift --bin uniffi-bindgen -- generate --language kotlin \
//!     --config apps/android/uniffi.toml --out-dir <out-dir> <library>
//! ```
//!
//! This is UniFFI's general generator. The Swift bindings have their own
//! program beside this one, the package's `main.rs`, because Swift needs a
//! module map that only that one writes. Both live in one package so that
//! the workspace has one build tool for bindings to list.

fn main() {
    uniffi::uniffi_bindgen_main()
}
