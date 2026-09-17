//! Generates the Kotlin and Swift bindings.
//!
//! UniFFI's generator reads the compiled library, so it has to be a binary in
//! this crate rather than an external tool — see `scripts/mobile-bindings.sh`.
fn main() {
    uniffi::uniffi_bindgen_main()
}
