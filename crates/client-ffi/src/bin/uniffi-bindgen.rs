//! The uniffi bindings generator, built from this crate so it always matches the
//! uniffi version the library uses. Run e.g.:
//!
//! ```bash
//! cargo run -p meerkly-client-ffi --bin uniffi-bindgen -- \
//!   generate --library target/release/libmeerkly.dylib \
//!   --language python --out-dir sdk/python
//! ```
fn main() {
    uniffi::uniffi_bindgen_main()
}
