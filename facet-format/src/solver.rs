extern crate alloc;

use alloc::borrow::Cow;
use alloc::sync::Arc;
use core::fmt;

use facet_solver::{KeyResult, Resolution, ResolutionHandle, Schema, Solver};

use crate::{FormatParser, ParseEvent};

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
    /// Parser error while reading events.
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
/// This uses save/restore to read ahead and determine the variant without
/// consuming the events permanently. After this returns, the parser position
/// is restored so the actual deserialization can proceed.
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

    // Save position and start recording events
    let save_point = parser.save();

    // Read through the structure, looking for field keys
    let result = solve_variant_inner(&mut solver, parser);

    // Restore position regardless of outcome
    parser.restore(save_point);

    match result {
        Ok(Some(handle)) => {
            let idx = handle.index();
            Ok(Some(SolveOutcome {
                schema,
                resolution_index: idx,
            }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Inner function that reads events and feeds field names to the solver.
fn solve_variant_inner<'de, 'a, P>(
    solver: &mut Solver<'a>,
    parser: &mut P,
) -> Result<Option<ResolutionHandle<'a>>, SolveVariantError<P::Error>>
where
    'de: 'a,
    P: FormatParser<'de>,
{
    let mut depth = 0i32;
    let mut in_struct = false;
    let mut expecting_value = false;

    loop {
        let event = parser
            .next_event()
            .map_err(SolveVariantError::from_parser)?;

        let Some(event) = event else {
            // EOF reached
            return Ok(None);
        };

        match event {
            ParseEvent::StructStart(_) => {
                depth += 1;
                if depth == 1 {
                    in_struct = true;
                }
            }
            ParseEvent::StructEnd => {
                depth -= 1;
                if depth == 0 {
                    // Done with top-level struct
                    return Ok(None);
                }
            }
            ParseEvent::SequenceStart(_) => {
                depth += 1;
            }
            ParseEvent::SequenceEnd => {
                depth -= 1;
            }
            ParseEvent::FieldKey(key) => {
                if depth == 1 && in_struct {
                    // Top-level field - feed to solver
                    if let Some(name) = key.name
                        && let Some(handle) = handle_key(solver, name)
                    {
                        return Ok(Some(handle));
                    }
                    expecting_value = true;
                }
            }
            ParseEvent::Scalar(_) | ParseEvent::OrderedField | ParseEvent::VariantTag(_) => {
                if expecting_value {
                    expecting_value = false;
                }
            }
        }
    }
}

fn handle_key<'a>(solver: &mut Solver<'a>, name: Cow<'a, str>) -> Option<ResolutionHandle<'a>> {
    match solver.see_key(name) {
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
