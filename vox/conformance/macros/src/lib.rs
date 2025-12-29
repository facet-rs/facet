//! Proc macros for rapace conformance tests.
//!
//! Provides the `#[conformance]` attribute for registering conformance tests.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;

/// Attribute macro for marking conformance tests.
///
/// # Example
///
/// ```ignore
/// #[conformance(name = "call.response_method_id_must_match", rules = "core.call.response.method-id")]
/// async fn response_method_id_must_match(peer: &mut Peer) -> TestResult {
///     // test implementation
/// }
///
/// // Multiple rules:
/// #[conformance(name = "call.response_msg_id_echo", rules = "core.call.response.msg-id, frame.msg-id.call-echo")]
/// async fn response_msg_id_echo(peer: &mut Peer) -> TestResult {
///     // test implementation
/// }
/// ```
#[proc_macro_attribute]
pub fn conformance(attr: TokenStream, item: TokenStream) -> TokenStream {
    conformance_impl(attr.into(), item.into()).into()
}

fn conformance_impl(attr: TokenStream2, item: TokenStream2) -> TokenStream2 {
    // Parse the attributes: name = "...", rules = "..."
    let attr_str = attr.to_string();

    let attrs = match parse_attrs(&attr_str) {
        Ok(a) => a,
        Err(e) => {
            return quote! {
                compile_error!(#e);
            };
        }
    };

    let test_name = match attrs.get("name") {
        Some(n) => n.clone(),
        None => {
            return quote! {
                compile_error!("conformance attribute requires 'name = \"category.test_name\"'");
            };
        }
    };

    let rules_str = match attrs.get("rules") {
        Some(r) => r.clone(),
        None => {
            return quote! {
                compile_error!("conformance attribute requires 'rules = \"rule1, rule2\"'");
            };
        }
    };

    // Parse comma-separated rules from the string
    let rules: Vec<&str> = rules_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if rules.is_empty() {
        return quote! {
            compile_error!("conformance attribute requires at least one rule");
        };
    }

    // Parse the function to get its name
    let item_str = item.to_string();

    // Extract function name (simple approach - find "fn <name>")
    let fn_name = match extract_fn_name(&item_str) {
        Some(name) => name,
        None => {
            return quote! {
                compile_error!("Could not extract function name from item");
            };
        }
    };

    let fn_name_ident = quote::format_ident!("{}", fn_name);
    let wrapper_name = quote::format_ident!("{}_wrapper", fn_name);
    let registration_name = quote::format_ident!("__CONFORMANCE_TEST_{}", fn_name.to_uppercase());

    quote! {
        #item

        fn #wrapper_name(peer: &mut crate::harness::Peer) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = crate::testcase::TestResult> + Send + '_>> {
            Box::pin(#fn_name_ident(peer))
        }

        ::inventory::submit! {
            crate::ConformanceTest {
                name: #test_name,
                rules: &[#(#rules),*],
                func: #wrapper_name,
            }
        }

        #[allow(dead_code)]
        const #registration_name: () = ();
    }
}

/// Parse key="value" pairs from the attribute string.
fn parse_attrs(s: &str) -> Result<HashMap<String, String>, String> {
    let mut result = HashMap::new();
    let s = s.trim();

    // Split by comma outside of quotes
    let mut current = String::new();
    let mut in_quotes = false;
    let mut parts = Vec::new();

    for c in s.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
            current.push(c);
        } else if c == ',' && !in_quotes {
            parts.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(c);
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Find the = sign
        let eq_pos = match part.find('=') {
            Some(p) => p,
            None => {
                return Err(format!(
                    "invalid attribute '{}': expected key = \"value\"",
                    part
                ));
            }
        };

        let key = part[..eq_pos].trim();
        let value = part[eq_pos + 1..].trim();

        // Value must be a quoted string
        if !value.starts_with('"') || !value.ends_with('"') {
            return Err(format!("attribute '{}' value must be a quoted string", key));
        }

        let value = &value[1..value.len() - 1];
        result.insert(key.to_string(), value.to_string());
    }

    Ok(result)
}

fn extract_fn_name(s: &str) -> Option<String> {
    // Find "fn " then extract the identifier
    let fn_pos = s.find("fn ")?;
    let after_fn = &s[fn_pos + 3..];
    let trimmed = after_fn.trim_start();

    // Find the end of the identifier (first non-ident char)
    let end = trimmed
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(trimmed.len());

    Some(trimmed[..end].to_string())
}
