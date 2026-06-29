#![forbid(unsafe_code)]
//! WebAssembly bindings for Snark playgrounds.

use wasm_bindgen::prelude::*;

/// Parse one playground request with Snark and return a JSON response.
#[wasm_bindgen(js_name = parseBundle)]
pub fn parse_bundle(request_json: &str) -> String {
    snark_playground::parse_bundle_json(request_json)
}
