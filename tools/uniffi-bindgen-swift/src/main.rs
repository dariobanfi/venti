//! Generates the Swift bindings for `venti-core` from its compiled library.
//!
//! ```text
//! cargo run -p uniffi-bindgen-swift -- <library> <out-dir> --swift-sources --headers
//! ```

fn main() {
    uniffi::uniffi_bindgen_swift()
}
