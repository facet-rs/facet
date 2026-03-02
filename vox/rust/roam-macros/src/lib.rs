//! Procedural macros for roam RPC service definitions.
//!
//! The `#[service]` macro generates everything needed for a roam RPC service.
//! All generation logic lives in `roam-macros-core` for testability.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;

// r[service-macro.is-source-of-truth]
/// Marks a trait as a roam RPC service and generates all service code.
///
/// # Generated Items
///
/// For a trait named `Calculator`, this generates:
/// - `mod calculator` containing:
///   - `pub use` of common types (Tx, Rx, RoamError, etc.)
///   - `mod method_id` with lazy method ID functions
///   - `trait Calculator` - the service trait
///   - `struct CalculatorDispatcher<H>` - server-side dispatcher
///   - `struct CalculatorClient` - client for making calls
///
/// # Example
///
/// ```ignore
/// #[roam::service]
/// trait Calculator {
///     async fn add(&self, a: i32, b: i32) -> i32;
/// }
/// ```
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = TokenStream2::from(item);

    let parsed = match roam_macros_core::parse(&input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    match roam_macros_core::generate_service(&parsed, &roam_macros_core::roam_crate()) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
