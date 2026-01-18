use proc_macro::TokenStream;
use quote::quote;
use unsynn::{LiteralString, Parse, ToTokens, TokenIter};

/// Register a migration function.
///
/// The version is automatically derived from the filename. For example,
/// a file named `m_2026_01_18_173711_create_users.rs` will have version
/// `2026_01_18_173711-create_users`.
///
/// # Example
///
/// ```ignore
/// // In file: src/migrations/m_2026_01_18_create_users.rs
/// #[dibs::migration]
/// async fn migrate(ctx: &mut MigrationContext) -> Result<()> {
///     ctx.execute("CREATE TABLE users (...)").await?;
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn migration(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Convert to proc_macro2 and create unsynn TokenIter
    let attr2: proc_macro2::TokenStream = attr.into();
    let mut tokens = TokenIter::new(attr2);

    // Version is optional - if not provided, it will be derived from filename
    let explicit_version = LiteralString::parse(&mut tokens).ok();

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

    let version_expr = if let Some(version) = explicit_version {
        let version_lit = version.to_token_stream();
        quote! { #version_lit }
    } else {
        // Derive version from filename at compile time
        // file!() returns something like "src/migrations/m_2026_01_18_173711_create_users.rs"
        // We extract the filename, strip the .rs and leading m_, then format as version
        quote! {
            {
                const FILE: &str = file!();
                // Extract just the filename
                const fn find_last_slash(s: &[u8]) -> usize {
                    let mut i = s.len();
                    while i > 0 {
                        i -= 1;
                        if s[i] == b'/' || s[i] == b'\\' {
                            return i + 1;
                        }
                    }
                    0
                }
                const SLASH_POS: usize = find_last_slash(FILE.as_bytes());
                const FILENAME: &str = unsafe {
                    // SAFETY: SLASH_POS is always a valid index
                    std::str::from_utf8_unchecked(FILE.as_bytes().split_at(SLASH_POS).1)
                };
                // Strip .rs extension and leading m_
                ::dibs::__derive_migration_version(FILENAME)
            }
        }
    };

    quote! {
        #item

        ::dibs::inventory::submit! {
            ::dibs::Migration {
                version: #version_expr,
                name: stringify!(#fn_ident),
                run: |ctx| Box::pin(#fn_ident(ctx)),
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
