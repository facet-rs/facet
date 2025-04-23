use facet_derive_parse::{GenericParam, GenericParams, ToTokens};

/// The name of a generic parameter
#[derive(Clone)]
pub enum GenericParamName {
    /// "a" but formatted as "'a"
    Lifetime(String),

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

struct WithBounds<'a>(&'a BoundedGenericParams);

struct WithoutBounds<'a>(&'a BoundedGenericParams);

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

    pub fn add(&self, param: BoundedGenericParam) -> Self {
        let mut params = self.params.clone();

        match &param.param {
            GenericParamName::Lifetime(_) => {
                // Lifetimes go first
                params.insert(0, param);
            }
            GenericParamName::Type(_) => {
                // Find the position after the last lifetime parameter
                let insert_position = params
                    .iter()
                    .position(|p| !matches!(p.param, GenericParamName::Lifetime(_)))
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
