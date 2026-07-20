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

/// vix Ring-2 IDE bindings (symbols, references, unresolved) for the playground:
/// occurrence highlighting, go-to-definition, rename. Uses vix's own embedded
/// grammar — independent of whatever bundle the session has prepared.
#[wasm_bindgen(js_name = vixBindings)]
pub fn vix_bindings(source: &str) -> String {
    vix::ide::bindings_json(source)
}

/// vix syntax highlighting: the embedded highlights query over the embedded
/// grammar — clients need no grammar assets at all.
#[wasm_bindgen(js_name = vixHighlights)]
pub fn vix_highlights(source: &str) -> String {
    vix::ide::highlights_json(source)
}
