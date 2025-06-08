use facet_macros_parse::{GenericParam, GenericParams, ToTokens, TokenStream};
use quote::quote;

use crate::LifetimeName;

/// The name of a generic parameter
#[derive(Clone)]
pub enum GenericParamName {
    /// "a" but formatted as "'a"
    Lifetime(LifetimeName),

    /// "T", formatted as "T"
    Type(TokenStream),

    /// "N", formatted as "N"
    Const(TokenStream),
}

/// The name of a generic parameter with bounds
#[derive(Clone)]
pub struct BoundedGenericParam {
    /// the parameter name
    pub param: GenericParamName,

    /// bounds like `'static`, or `Send + Sync`, etc. — None if no bounds
    pub bounds: Option<TokenStream>,
}

/// Stores different representations of generic parameters for implementing traits.
///
/// This structure separates generic parameters into different formats needed when
/// generating trait implementations.
#[derive(Clone)]
pub struct BoundedGenericParams {
    /// Collection of generic parameters with their bounds
    pub params: Vec<BoundedGenericParam>,
}

/// Display wrapper that shows generic parameters with their bounds
///
/// This is used to format generic parameters for display purposes,
/// including both the parameter names and their bounds (if any).
///
/// # Example
///
/// For a parameter like `T: Clone`, this will display `<T: Clone>`.
pub struct WithBounds<'a>(&'a BoundedGenericParams);

/// Display wrapper that shows generic parameters without their bounds
///
/// This is used to format just the parameter names for display purposes,
/// omitting any bounds information.
///
/// # Example
///
/// For a parameter like `T: Clone`, this will display just `<T>`.
pub struct WithoutBounds<'a>(&'a BoundedGenericParams);

/// Display wrapper that outputs generic parameters as a PhantomData
///
/// This is used to format generic parameters as a PhantomData type
/// for use in trait implementations.
///
/// # Example
///
/// For parameters `<'a, T, const N: usize>`, this will display
/// `::core::marker::PhantomData<(*mut &'__facet (), T, [u32; N])>`.
pub struct AsPhantomData<'a>(&'a BoundedGenericParams);

impl quote::ToTokens for AsPhantomData<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut temp = TokenStream::new();

        {
            #[expect(unused)]
            let tokens = ();

            // Track if we've written anything to handle commas correctly
            let mut first_param = true;

            // Generate all parameters in the tuple
            for param in &self.0.params {
                if !first_param {
                    temp.extend(quote! { , });
                }

                match &param.param {
                    GenericParamName::Lifetime(name) => {
                        temp.extend(quote! { *mut &#name () });
                    }
                    GenericParamName::Type(name) => {
                        temp.extend(quote! { #name });
                    }
                    GenericParamName::Const(name) => {
                        temp.extend(quote! { [u32; #name] });
                    }
                }

                first_param = false;
            }

            // If no parameters at all, add a unit type to make the PhantomData valid
            if first_param {
                temp.extend(quote! { () });
            }
        }
        tokens.extend(quote! {
            ::core::marker::PhantomData<(#temp)>
        })
    }
}

impl BoundedGenericParams {
    pub fn as_phantom_data(&self) -> AsPhantomData<'_> {
        AsPhantomData(self)
    }
}

impl quote::ToTokens for WithBounds<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.0.params.is_empty() {
            return;
        }

        tokens.extend(quote! {
            <
        });

        for (i, param) in self.0.params.iter().enumerate() {
            if i > 0 {
                tokens.extend(quote! { , });
            }

            match &param.param {
                GenericParamName::Lifetime(name) => {
                    tokens.extend(quote! { #name });
                }
                GenericParamName::Type(name) => {
                    tokens.extend(quote! { #name });
                }
                GenericParamName::Const(name) => {
                    tokens.extend(quote! { const #name });
                }
            }

            // Add bounds if they exist
            if let Some(bounds) = &param.bounds {
                tokens.extend(quote! { : #bounds });
            }
        }

        tokens.extend(quote! {
            >
        });
    }
}

impl quote::ToTokens for WithoutBounds<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.0.params.is_empty() {
            return;
        }

        tokens.extend(quote! {
            <
        });

        for (i, param) in self.0.params.iter().enumerate() {
            if i > 0 {
                tokens.extend(quote! { , });
            }

            match &param.param {
                GenericParamName::Lifetime(name) => {
                    tokens.extend(quote! { #name });
                }
                GenericParamName::Type(name) => {
                    tokens.extend(quote! { #name });
                }
                GenericParamName::Const(name) => {
                    tokens.extend(quote! { #name });
                }
            }
        }

        tokens.extend(quote! {
            >
        });
    }
}

impl BoundedGenericParams {
    pub fn display_with_bounds(&self) -> WithBounds<'_> {
        WithBounds(self)
    }

    pub fn display_without_bounds(&self) -> WithoutBounds<'_> {
        WithoutBounds(self)
    }

    /// Returns a display wrapper that formats generic parameters as a PhantomData
    ///
    /// This is a convenience method for generating PhantomData expressions
    /// for use in trait implementations.
    ///
    /// # Example
    ///
    /// For generic parameters `<'a, T, const N: usize>`, this returns a wrapper that
    /// when displayed produces:
    /// `::core::marker::PhantomData<(*mut &'a (), T, [u32; N])>`
    pub fn display_as_phantom_data(&self) -> AsPhantomData<'_> {
        AsPhantomData(self)
    }

    pub fn with(&self, param: BoundedGenericParam) -> Self {
        let mut params = self.params.clone();

        match &param.param {
            GenericParamName::Lifetime(_) => {
                // Find the position after the last lifetime parameter
                let insert_position = params
                    .iter()
                    .position(|p| !matches!(p.param, GenericParamName::Lifetime(_)))
                    .unwrap_or(params.len());

                params.insert(insert_position, param);
            }
            GenericParamName::Type(_) => {
                // Find the position after the last type parameter but before any const parameters
                let after_lifetimes = params
                    .iter()
                    .position(|p| !matches!(p.param, GenericParamName::Lifetime(_)))
                    .unwrap_or(params.len());

                let insert_position = params[after_lifetimes..]
                    .iter()
                    .position(|p| matches!(p.param, GenericParamName::Const(_)))
                    .map(|pos| pos + after_lifetimes)
                    .unwrap_or(params.len());

                params.insert(insert_position, param);
            }
            GenericParamName::Const(_) => {
                // Constants go at the end
                params.push(param);
            }
        }

        Self { params }
    }

    /// Adds a new lifetime parameter with the given name without bounds
    ///
    /// This is a convenience method for adding a lifetime parameter
    /// that's commonly used in trait implementations.
    pub fn with_lifetime(&self, name: LifetimeName) -> Self {
        self.with(BoundedGenericParam {
            param: GenericParamName::Lifetime(name),
            bounds: None,
        })
    }

    /// Adds a new type parameter with the given name without bounds
    ///
    /// This is a convenience method for adding a type parameter
    /// that's commonly used in trait implementations.
    pub fn with_type(&self, name: TokenStream) -> Self {
        self.with(BoundedGenericParam {
            param: GenericParamName::Type(name),
            bounds: None,
        })
    }
}

impl BoundedGenericParams {
    /// Parses generic parameters into separate components for implementing traits.
    ///
    /// This method takes a generic parameter declaration and populates the BoundedGenericParams struct
    /// with different representations of the generic parameters needed for code generation.
    ///
    /// # Examples
    ///
    /// For a type like `struct Example<T: Clone, 'a, const N: usize>`, this would populate:
    /// params with entries for each parameter and their bounds.
    pub fn parse(generics: Option<&GenericParams>) -> Self {
        let Some(generics) = generics else {
            return Self { params: Vec::new() };
        };

        let mut params = Vec::new();

        for param in generics.params.0.iter() {
            match &param.value {
                GenericParam::Type {
                    name,
                    bounds,
                    default: _,
                } => {
                    params.push(BoundedGenericParam {
                        param: GenericParamName::Type(name.to_token_stream()),
                        bounds: bounds
                            .as_ref()
                            .map(|bounds| bounds.second.to_token_stream()),
                    });
                }
                GenericParam::Lifetime { name, bounds } => {
                    params.push(BoundedGenericParam {
                        param: GenericParamName::Lifetime(LifetimeName(name.name.clone())),
                        bounds: bounds
                            .as_ref()
                            .map(|bounds| bounds.second.to_token_stream()),
                    });
                }
                GenericParam::Const {
                    _const: _,
                    name,
                    _colon: _,
                    typ,
                    default: _,
                } => {
                    params.push(BoundedGenericParam {
                        param: GenericParamName::Const(name.to_token_stream()),
                        bounds: Some(typ.to_token_stream()),
                    });
                }
            }
        }

        Self { params }
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedGenericParam, BoundedGenericParams, GenericParamName};
    use crate::LifetimeName;
    use quote::{ToTokens as _, quote};

    // Helper to render ToTokens implementors to string for comparison
    fn render_to_string<T: quote::ToTokens>(t: T) -> String {
        quote!(#t).to_string()
    }

    #[test]
    fn test_empty_generic_params() {
        let p = BoundedGenericParams { params: vec![] };
        assert_eq!(render_to_string(p.display_with_bounds()), "");
        assert_eq!(render_to_string(p.display_without_bounds()), "");
    }

    #[test]
    fn print_multiple_generic_params() {
        let p = BoundedGenericParams {
            params: vec![
                BoundedGenericParam {
                    bounds: Some(quote! { 'static }),
                    param: GenericParamName::Lifetime(LifetimeName(quote::format_ident!("a"))),
                },
                BoundedGenericParam {
                    bounds: Some(quote! { Clone + Debug }),
                    param: GenericParamName::Type(quote! { T }),
                },
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Type(quote! { U }),
                },
                BoundedGenericParam {
                    bounds: Some(quote! { usize }), // Const params bounds are types
                    param: GenericParamName::Const(quote! { N }),
                },
            ],
        };
        // Check display with bounds
        let expected_with_bounds = quote! { <'a : 'static, T : Clone + Debug, U, const N : usize> };
        assert_eq!(
            p.display_with_bounds().to_token_stream().to_string(),
            expected_with_bounds.to_string()
        );

        // Check display without bounds
        let expected_without_bounds = quote! { <'a, T, U, N> }; // Note: const param N doesn't show `const` or type here
        assert_eq!(
            p.display_without_bounds().to_token_stream().to_string(),
            expected_without_bounds.to_string()
        );
    }

    #[test]
    fn test_add_mixed_parameters() {
        // Create a complex example with all parameter types
        let mut params = BoundedGenericParams { params: vec![] };

        // Add parameters in different order to test sorting
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type(quote! { T }),
        });

        params = params.with(BoundedGenericParam {
            bounds: Some(quote! { usize }), // Const bounds are types
            param: GenericParamName::Const(quote! { N }),
        });

        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime(LifetimeName(quote::format_ident!("a"))),
        });

        params = params.with(BoundedGenericParam {
            bounds: Some(quote! { Clone }),
            param: GenericParamName::Type(quote! { U }),
        });

        params = params.with(BoundedGenericParam {
            bounds: Some(quote! { 'static }),
            param: GenericParamName::Lifetime(LifetimeName(quote::format_ident!("b"))),
        });

        params = params.with(BoundedGenericParam {
            bounds: Some(quote! { u8 }), // Const bounds are types
            param: GenericParamName::Const(quote! { M }),
        });

        // Expected order: lifetimes first, then types, then consts
        let expected_without_bounds = quote! { <'a, 'b, T, U, N, M> };
        // Compare string representations for robust assertion
        assert_eq!(
            params
                .display_without_bounds()
                .to_token_stream()
                .to_string(),
            expected_without_bounds.to_string()
        );

        let expected_with_bounds =
            quote! { <'a, 'b : 'static, T, U : Clone, const N : usize, const M : u8> };
        // Compare string representations for robust assertion
        assert_eq!(
            params.display_with_bounds().to_token_stream().to_string(),
            expected_with_bounds.to_string()
        );
    }

    #[test]
    fn test_phantom_data_formatting() {
        // Empty params should have PhantomData with a unit type
        let empty = BoundedGenericParams { params: vec![] };
        assert_eq!(
            render_to_string(empty.display_as_phantom_data()),
            ":: core :: marker :: PhantomData < (()) >"
        );

        // Single lifetime
        let lifetime = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                param: GenericParamName::Lifetime(LifetimeName(quote::format_ident!("a"))),
                bounds: None,
            }],
        };
        assert_eq!(
            render_to_string(lifetime.display_as_phantom_data()),
            ":: core :: marker :: PhantomData < (* mut & 'a ()) >"
        );

        // Single type
        let type_param = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                param: GenericParamName::Type(quote! { T }),
                bounds: None,
            }],
        };
        assert_eq!(
            render_to_string(type_param.display_as_phantom_data()),
            ":: core :: marker :: PhantomData < (T) >"
        );

        // Single const
        let const_param = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                param: GenericParamName::Const(quote! { N }),
                bounds: None, // Bounds are irrelevant for PhantomData formatting
            }],
        };
        assert_eq!(
            render_to_string(const_param.display_as_phantom_data()),
            ":: core :: marker :: PhantomData < ([u32 ; N]) >"
        );

        // Complex mix of params
        let mixed = BoundedGenericParams {
            params: vec![
                BoundedGenericParam {
                    param: GenericParamName::Lifetime(LifetimeName(quote::format_ident!("a"))),
                    bounds: None,
                },
                BoundedGenericParam {
                    param: GenericParamName::Type(quote! { T }),
                    bounds: Some(quote! { Clone }), // Bounds irrelevant here
                },
                BoundedGenericParam {
                    param: GenericParamName::Const(quote! { N }),
                    bounds: Some(quote! { usize }), // Bounds irrelevant here
                },
            ],
        };
        let actual_tokens = mixed.display_as_phantom_data();
        let expected_tokens = quote! {
            ::core::marker::PhantomData<(*mut &'a (), T, [u32; N])>
        };
        assert_eq!(
            actual_tokens.to_token_stream().to_string(),
            expected_tokens.to_string()
        );
    }
}
