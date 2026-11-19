//! Generates the Kotlin bindings for `venti-core` from its compiled library.
//!
//! ```text
//! cargo run -p uniffi-bindgen -- generate --language kotlin \
//!     --config apps/android/uniffi.toml --out-dir <out-dir> <library>
//! ```
//!
//! This is UniFFI's general generator. The Swift bindings have their own
//! tool beside this one, `uniffi-bindgen-swift`, because Swift needs a
//! module map that only that one writes.

fn main() {
    uniffi::uniffi_bindgen_main()
}
