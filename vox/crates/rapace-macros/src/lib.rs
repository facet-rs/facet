//! rapace-macros: Proc macros for rapace RPC.
//!
//! Provides `#[rapace::service]` which generates:
//! - Client stubs with async methods
//! - Server dispatch by method_id

use proc_macro::TokenStream;

/// Generates RPC client and server from a trait definition.
///
/// ```ignore
/// #[rapace::service]
/// trait Calculator {
///     async fn add(&self, a: i32, b: i32) -> i32;
/// }
/// ```
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // TODO: implement service macro
    item
}
