use proc_macro::TokenStream;
use quote::quote;
use unsynn::{LiteralString, Parse, ToTokens, TokenIter};

/// Register a migration function.
///
/// # Example
///
/// ```ignore
/// #[dibs::migration("2026-01-17-create-users")]
/// async fn create_users(ctx: &mut MigrationContext) -> Result<()> {
///     ctx.execute("CREATE TABLE users (...)").await?;
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn migration(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Convert to proc_macro2 and create unsynn TokenIter
    let attr2: proc_macro2::TokenStream = attr.into();
    let mut tokens = TokenIter::new(attr2);

    let version = match LiteralString::parse(&mut tokens) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("expected string literal for migration version: {e}");
            return quote! { compile_error!(#msg); }.into();
        }
    };

    let version_lit = version.to_token_stream();

    let item: proc_macro2::TokenStream = item.into();

    // Extract function name from the item
    let item_str = item.to_string();
    let fn_name = match extract_fn_name(&item_str) {
        Some(name) => name,
        None => {
            return quote! { compile_error!("expected function"); }.into();
        }
    };
    let fn_ident = quote::format_ident!("{}", fn_name);

    quote! {
        #item

        ::dibs::inventory::submit! {
            ::dibs::Migration {
                version: #version_lit,
                name: stringify!(#fn_ident),
                run: |ctx| Box::pin(#fn_ident(ctx)),
                // Use CARGO_MANIFEST_DIR for the crate root, file!() gives path from there
                // But file!() already includes the full path from workspace root in workspaces
                // So we just use file!() and resolve it at runtime relative to the manifest dir
                source_file: (env!("CARGO_MANIFEST_DIR"), file!()),
            }
        }
    }
    .into()
}

fn extract_fn_name(s: &str) -> Option<&str> {
    // Simple extraction: find "fn " and take the next identifier
    let idx = s.find("fn ")?;
    let rest = &s[idx + 3..].trim_start();
    let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    Some(&rest[..end])
}
