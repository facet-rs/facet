#![forbid(unsafe_code)]
//! WebAssembly bindings for Snark playgrounds.

use wasm_bindgen::prelude::*;

/// Prepared Snark playground session for one grammar bundle.
#[wasm_bindgen]
pub struct SnarkPlaygroundSession {
    inner: snark_playground::PlaygroundSession,
}

#[wasm_bindgen]
impl SnarkPlaygroundSession {
    /// Prepare one grammar bundle for repeated parsing.
    #[wasm_bindgen(constructor)]
    pub fn new(request_json: &str) -> Result<SnarkPlaygroundSession, JsValue> {
        let inner = snark_playground::PlaygroundSession::prepare_json(request_json)
            .map_err(|message| JsValue::from_str(&message))?;
        Ok(Self { inner })
    }

    /// Parse one input with the prepared bundle and return a JSON response.
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(&mut self, request_json: &str) -> String {
        self.inner.parse_json(request_json)
    }

    /// Reparse one edited input with the prepared bundle and return a JSON response.
    #[wasm_bindgen(js_name = reparse)]
    pub fn reparse(&mut self, request_json: &str) -> String {
        self.inner.parse_json(request_json)
    }
}

/// Parse one playground request with Snark and return a JSON response.
#[wasm_bindgen(js_name = parseBundle)]
pub fn parse_bundle(request_json: &str) -> String {
    snark_playground::parse_bundle_json(request_json)
}
