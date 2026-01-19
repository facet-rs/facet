use wasm_bindgen::prelude::*;
use marq::{render, RenderOptions};

/// Render markdown to HTML with syntax highlighting for styx code blocks
#[wasm_bindgen]
pub async fn render_markdown(input: &str) -> Result<String, JsValue> {
    let opts = RenderOptions::default();
    let doc = render(input, &opts)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(doc.html)
}
