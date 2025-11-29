use facet_macros_parse::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use quote::quote;

/// Entry point for the on_error attribute macro.
///
/// Usage: `#[on_error(self.poison_and_cleanup())]`
///
/// This wraps methods that return `Result<_, E>` to run cleanup code on error.
/// For methods returning `Result<&mut Self, E>`, it properly handles the borrow
/// by discarding the returned reference and returning a fresh `Ok(self)`.
///
/// The macro generates two methods:
/// - `__method_name_inner`: contains the original body
/// - `method_name`: wrapper that calls inner and handles errors
pub fn on_error(attr: TokenStream, item: TokenStream) -> TokenStream {
    // The attribute contains the cleanup expression
    let cleanup_expr = attr;

    let tokens: Vec<TokenTree> = item.into_iter().collect();

    // Find the function name - it's the identifier after "fn"
    let fn_pos = tokens
        .iter()
        .position(|tt| matches!(tt, TokenTree::Ident(id) if id.to_string() == "fn"))
        .expect("Method must have 'fn' keyword");

    let fn_name = if let TokenTree::Ident(id) = &tokens[fn_pos + 1] {
        id.clone()
    } else {
        panic!("Expected function name after 'fn'");
    };

    // Find the body - it's the last BraceGroup
    let body_idx = tokens
        .iter()
        .rposition(|tt| matches!(tt, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace))
        .expect("Method must have a body");

    let body = if let TokenTree::Group(g) = &tokens[body_idx] {
        g.clone()
    } else {
        unreachable!()
    };

    // Check if return type contains `&mut Self`
    let returns_mut_self = {
        let before_body: TokenStream = tokens[..body_idx].iter().cloned().collect();
        let s = before_body.to_string();
        if let Some(arrow_pos) = s.rfind("->") {
            let ret_type = &s[arrow_pos..];
            ret_type.contains("& mut Self") || ret_type.contains("&mut Self")
        } else {
            false
        }
    };

    // Generate inner method name
    let inner_name = Ident::new(&format!("__{}_inner", fn_name), Span::call_site());

    // Build the signature for the inner method (everything before the body, with renamed fn)
    let mut inner_sig_tokens: Vec<TokenTree> = Vec::new();
    for (i, tt) in tokens[..body_idx].iter().enumerate() {
        if i == fn_pos + 1 {
            // Replace function name with inner name
            inner_sig_tokens.push(TokenTree::Ident(inner_name.clone()));
        } else {
            inner_sig_tokens.push(tt.clone());
        }
    }
    let inner_sig: TokenStream = inner_sig_tokens.into_iter().collect();

    // Build the wrapper body
    let wrapper_body = if returns_mut_self {
        quote! {
            {
                match self.#inner_name() {
                    Ok(_discarded) => Ok(self),
                    Err(__e) => {
                        #cleanup_expr;
                        Err(__e)
                    }
                }
            }
        }
    } else {
        quote! {
            {
                let __result = self.#inner_name();
                if __result.is_err() {
                    #cleanup_expr;
                }
                __result
            }
        }
    };

    // Build the wrapper signature (original, without doc comments to avoid duplication)
    let wrapper_sig: TokenStream = tokens[..body_idx].iter().cloned().collect();

    // Combine: inner method + wrapper method
    let wrapper_body_group = TokenTree::Group(Group::new(Delimiter::Brace, wrapper_body));

    quote! {
        #[doc(hidden)]
        #[inline(always)]
        #inner_sig
        #body

        #wrapper_sig
        #wrapper_body_group
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_error_basic() {
        let attr = quote! { self.cleanup() };
        let item = quote! {
            pub fn do_something(&mut self) -> Result<i32, Error> {
                self.inner_work()?;
                Ok(42)
            }
        };

        let result = on_error(attr, item);
        let result_str = result.to_string();

        // Should contain the inner method
        assert!(result_str.contains("__do_something_inner"));
        assert!(result_str.contains("self . cleanup"));
    }

    #[test]
    fn test_on_error_mut_self_return() {
        let attr = quote! { self.poison_and_cleanup() };
        let item = quote! {
            pub fn begin_some(&mut self) -> Result<&mut Self, ReflectError> {
                self.require_active()?;
                Ok(self)
            }
        };

        let result = on_error(attr, item);
        let result_str = result.to_string();

        // Should detect &mut Self return and use the special handling
        assert!(result_str.contains("__begin_some_inner"));
        assert!(result_str.contains("_discarded"));
        assert!(result_str.contains("Ok (self)"));
    }
}
