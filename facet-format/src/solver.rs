extern crate alloc;

use alloc::sync::Arc;
use core::fmt;

use facet_solver::{KeyResult, Resolution, ResolutionHandle, Schema, Solver};

use crate::{FieldEvidence, FormatParser, ProbeStream};

/// High-level outcome from solving an untagged enum.
pub struct SolveOutcome {
    /// The schema that was used for solving
    pub schema: Arc<Schema>,
    /// Index of the chosen resolution in `schema.resolutions()`
    pub resolution_index: usize,
}

/// Error when variant solving fails.
#[derive(Debug)]
pub enum SolveVariantError<E> {
    /// No variant matched the evidence.
    NoMatch,
    /// Parser error while collecting evidence.
    Parser(E),
    /// Schema construction error.
    SchemaError(facet_solver::SchemaError),
}

impl<E> SolveVariantError<E> {
    /// Wrap a parser error into [`SolveVariantError::Parser`].
    pub const fn from_parser(e: E) -> Self {
        Self::Parser(e)
    }
}

impl<E: fmt::Display> fmt::Display for SolveVariantError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMatch => write!(f, "No variant matched"),
            Self::Parser(e) => write!(f, "Parser error: {}", e),
            Self::SchemaError(e) => write!(f, "Schema error: {}", e),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> core::error::Error for SolveVariantError<E> {}

/// Attempt to solve which enum variant matches the input.
///
/// Returns `Ok(Some(_))` if a unique variant was found, `Ok(None)` if
/// no variant matched, or `Err(_)` on error.
pub fn solve_variant<'de, P>(
    shape: &'static facet_core::Shape,
    parser: &mut P,
) -> Result<Option<SolveOutcome>, SolveVariantError<P::Error>>
where
    P: FormatParser<'de>,
{
    let schema = Arc::new(Schema::build_auto(shape)?);
    let mut solver = Solver::new(&schema);
    let mut probe = parser
        .begin_probe()
        .map_err(SolveVariantError::from_parser)?;

    while let Some(field) = probe.next().map_err(SolveVariantError::from_parser)? {
        if let Some(handle) = handle_key(&mut solver, field) {
            let idx = handle.index();
            return Ok(Some(SolveOutcome {
                schema,
                resolution_index: idx,
            }));
        }
    }

    Ok(None)
}

fn handle_key<'a>(
    solver: &mut Solver<'a>,
    field: FieldEvidence<'a>,
) -> Option<ResolutionHandle<'a>> {
    let owned_name = field.name.into_owned();
    match solver.see_key(owned_name) {
        KeyResult::Solved(handle) => Some(handle),
        KeyResult::Unknown | KeyResult::Unambiguous { .. } | KeyResult::Ambiguous { .. } => None,
    }
}

impl<E> From<facet_solver::SchemaError> for SolveVariantError<E> {
    fn from(e: facet_solver::SchemaError) -> Self {
        Self::SchemaError(e)
    }
}
impl SolveOutcome {
    /// Resolve the selected configuration reference.
    pub fn resolution(&self) -> &Resolution {
        &self.schema.resolutions()[self.resolution_index]
    }
}
