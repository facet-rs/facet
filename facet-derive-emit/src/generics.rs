use facet_derive_parse::{GenericParam, GenericParams, ToTokens};

/// The name of a generic parameter
#[derive(Clone)]
pub enum GenericParamName {
    /// "a" but formatted as "'a"
    Lifetime(String), // this could be Cow or a small string, honestly

    /// "T", formatted as "T"
    Type(String),

    /// "N", formatted as "const N"
    Const(String),
}

impl std::fmt::Display for GenericParamName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenericParamName::Lifetime(name) => write!(f, "'{}", name),
            GenericParamName::Type(name) => write!(f, "{}", name),
            GenericParamName::Const(name) => write!(f, "const {}", name),
        }
    }
}

/// The name of a generic parameter with bounds
///
/// # Examples
///
/// ```
/// // Lifetime parameter with 'static bound
/// BoundedGenericParam {
///     param: GenericParamName::Lifetime("a".to_string()),
///     bounds: Some("'static".to_string())
/// }
///
/// // Type parameter with Send + Sync bounds
/// BoundedGenericParam {
///     param: GenericParamName::Type("T".to_string()),
///     bounds: Some("Send + Sync".to_string())
/// }
///
/// // Const parameter with usize bound
/// BoundedGenericParam {
///     param: GenericParamName::Const("N".to_string()),
///     bounds: Some("usize".to_string())
/// }
///
/// // Type parameter with no bounds
/// BoundedGenericParam {
///     param: GenericParamName::Type("U".to_string()),
///     bounds: None
/// }
/// ```
#[derive(Clone)]
pub struct BoundedGenericParam {
    /// the parameter name
    pub param: GenericParamName,

    /// bounds like `'static`, or `Send + Sync`, etc. — None if no bounds
    pub bounds: Option<String>,
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

impl std::fmt::Display for WithBounds<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.params.is_empty() {
            return Ok(());
        }

        write!(f, "<")?;
        for (i, param) in self.0.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param.param)?;
            if let Some(bounds) = &param.bounds {
                write!(f, ": {}", bounds)?;
            }
        }
        write!(f, ">")
    }
}

impl std::fmt::Display for WithoutBounds<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.params.is_empty() {
            return Ok(());
        }

        write!(f, "<")?;
        for (i, param) in self.0.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param.param)?;
        }
        write!(f, ">")
    }
}

impl BoundedGenericParams {
    pub fn display_with_bounds(&self) -> WithBounds<'_> {
        WithBounds(self)
    }

    pub fn display_without_bounds(&self) -> WithoutBounds<'_> {
        WithoutBounds(self)
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
    ///
    /// These can then be used to generate code like:
    /// ```
    /// impl<T: Clone, 'a, const N: usize> SomeTrait for Example<T, 'a, N> {
    ///     // implementation
    /// }
    /// ```
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
                    let name_str = name.to_string();
                    let bounds_str = bounds
                        .as_ref()
                        .map(|bounds| bounds.second.tokens_to_string());
                    params.push(BoundedGenericParam {
                        param: GenericParamName::Type(name_str),
                        bounds: bounds_str,
                    });
                }
                GenericParam::Lifetime { name, bounds } => {
                    let name_str = name.to_string();
                    let bounds_str = bounds
                        .as_ref()
                        .map(|bounds| bounds.second.tokens_to_string());
                    params.push(BoundedGenericParam {
                        param: GenericParamName::Lifetime(name_str),
                        bounds: bounds_str,
                    });
                }
                GenericParam::Const {
                    _const: _,
                    name,
                    _colon: _,
                    typ,
                    default: _,
                } => {
                    let name_str = name.to_string();
                    params.push(BoundedGenericParam {
                        param: GenericParamName::Const(name_str),
                        bounds: Some(typ.tokens_to_string()),
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

    #[test]
    fn test_empty_generic_params() {
        let p = BoundedGenericParams { params: vec![] };
        assert_eq!(p.display_with_bounds().to_string(), "");
        assert_eq!(p.display_without_bounds().to_string(), "");
    }

    #[test]
    fn print_type_no_bounds() {
        let p = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: None,
                param: GenericParamName::Type("T".to_string()),
            }],
        };
        assert_eq!(p.display_with_bounds().to_string(), "<T>");
        assert_eq!(p.display_without_bounds().to_string(), "<T>");
    }

    #[test]
    fn print_type_with_clone_bound() {
        let p = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: Some("Clone".to_string()),
                param: GenericParamName::Type("T".to_string()),
            }],
        };
        assert_eq!(p.display_with_bounds().to_string(), "<T: Clone>");
        assert_eq!(p.display_without_bounds().to_string(), "<T>");
    }

    #[test]
    fn print_lifetime_no_bounds() {
        let p = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: None,
                param: GenericParamName::Lifetime("a".to_string()),
            }],
        };
        assert_eq!(p.display_with_bounds().to_string(), "<'a>");
        assert_eq!(p.display_without_bounds().to_string(), "<'a>");
    }

    #[test]
    fn print_lifetime_with_static_bound() {
        let p = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: Some("'static".to_string()),
                param: GenericParamName::Lifetime("a".to_string()),
            }],
        };
        assert_eq!(p.display_with_bounds().to_string(), "<'a: 'static>");
        assert_eq!(p.display_without_bounds().to_string(), "<'a>");
    }

    #[test]
    fn print_const_no_bounds() {
        let p = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: None,
                param: GenericParamName::Const("N".to_string()),
            }],
        };
        assert_eq!(p.display_with_bounds().to_string(), "<const N>");
        assert_eq!(p.display_without_bounds().to_string(), "<const N>");
    }

    #[test]
    fn print_const_with_usize_bound() {
        let p = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: Some("usize".to_string()),
                param: GenericParamName::Const("N".to_string()),
            }],
        };
        assert_eq!(p.display_with_bounds().to_string(), "<const N: usize>");
        assert_eq!(p.display_without_bounds().to_string(), "<const N>");
    }

    #[test]
    fn print_multiple_generic_params() {
        let p = BoundedGenericParams {
            params: vec![
                BoundedGenericParam {
                    bounds: Some("'static".to_string()),
                    param: GenericParamName::Lifetime("a".to_string()),
                },
                BoundedGenericParam {
                    bounds: Some("Clone + Debug".to_string()),
                    param: GenericParamName::Type("T".to_string()),
                },
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Type("U".to_string()),
                },
                BoundedGenericParam {
                    bounds: Some("usize".to_string()),
                    param: GenericParamName::Const("N".to_string()),
                },
            ],
        };
        assert_eq!(
            p.display_with_bounds().to_string(),
            "<'a: 'static, T: Clone + Debug, U, const N: usize>"
        );
        assert_eq!(
            p.display_without_bounds().to_string(),
            "<'a, T, U, const N>"
        );
    }

    #[test]
    fn test_add_lifetime_parameters() {
        // Starting from empty params
        let mut params = BoundedGenericParams { params: vec![] };

        // Add a lifetime parameter 'a
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime("a".to_string()),
        });

        // Add another lifetime parameter 'b
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime("b".to_string()),
        });

        assert_eq!(params.display_without_bounds().to_string(), "<'a, 'b>");

        // Starting from params with existing types and consts
        let mut params = BoundedGenericParams {
            params: vec![
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Type("T".to_string()),
                },
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Const("N".to_string()),
                },
            ],
        };

        // Add a lifetime parameter - should be placed before types
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime("a".to_string()),
        });

        assert_eq!(
            params.display_without_bounds().to_string(),
            "<'a, T, const N>"
        );
    }

    #[test]
    fn test_add_type_parameters() {
        // Starting from empty params
        let mut params = BoundedGenericParams { params: vec![] };

        // Add a type parameter T
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type("T".to_string()),
        });

        // Add another type parameter U
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type("U".to_string()),
        });

        assert_eq!(params.display_without_bounds().to_string(), "<T, U>");

        // Starting from params with existing lifetimes
        let mut params = BoundedGenericParams {
            params: vec![BoundedGenericParam {
                bounds: None,
                param: GenericParamName::Lifetime("a".to_string()),
            }],
        };

        // Add a type parameter - should be placed after lifetimes
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type("T".to_string()),
        });

        assert_eq!(params.display_without_bounds().to_string(), "<'a, T>");

        // Starting from params with existing lifetimes and consts
        let mut params = BoundedGenericParams {
            params: vec![
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Lifetime("a".to_string()),
                },
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Const("N".to_string()),
                },
            ],
        };

        // Add a type parameter - should be placed between lifetimes and consts
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type("T".to_string()),
        });

        assert_eq!(
            params.display_without_bounds().to_string(),
            "<'a, T, const N>"
        );
    }

    #[test]
    fn test_add_const_parameters() {
        // Starting from empty params
        let mut params = BoundedGenericParams { params: vec![] };

        // Add a const parameter N
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Const("N".to_string()),
        });

        // Add another const parameter M
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Const("M".to_string()),
        });

        assert_eq!(
            params.display_without_bounds().to_string(),
            "<const N, const M>"
        );

        // Starting from params with existing lifetimes and types
        let mut params = BoundedGenericParams {
            params: vec![
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Lifetime("a".to_string()),
                },
                BoundedGenericParam {
                    bounds: None,
                    param: GenericParamName::Type("T".to_string()),
                },
            ],
        };

        // Add a const parameter - should be placed at the end
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Const("N".to_string()),
        });

        assert_eq!(
            params.display_without_bounds().to_string(),
            "<'a, T, const N>"
        );
    }

    #[test]
    fn test_add_mixed_parameters() {
        // Create a complex example with all parameter types
        let mut params = BoundedGenericParams { params: vec![] };

        // Add parameters in different order to test sorting
        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type("T".to_string()),
        });

        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Const("N".to_string()),
        });

        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime("a".to_string()),
        });

        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Type("U".to_string()),
        });

        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime("b".to_string()),
        });

        params = params.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Const("M".to_string()),
        });

        // Expected order: lifetimes first, then types, then consts
        assert_eq!(
            params.display_without_bounds().to_string(),
            "<'a, 'b, T, U, const N, const M>"
        );
    }
}
