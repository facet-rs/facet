use crate::error::{ArgsError, ArgsErrorKind};
use facet_core::{Type, UserType};
use facet_deserialize::{PopReason, Span, StackRunner};
use facet_reflect::{ReflectError, Wip};

/// Applies defaults to uninitialized fields
///
/// /// This function leverages the `StackRunner` from `facet-deserialize` to apply default values
/// to fields that have the `DEFAULT` flag. It preserves the special handling for boolean fields
/// that's specific to CLI argument parsing.
///
/// # Arguments
///
/// * `wip` - A working-in-progress value to apply defaults to
///
/// # Returns
///
/// The `wip` with defaults applied to non-boolean fields
pub fn apply_field_defaults(wip: Wip<'_>) -> Result<Wip<'_>, ArgsError> {
    // Guard clause for non-struct types
    if !matches!(wip.shape().ty, Type::User(UserType::Struct(_))) {
        return Ok(wip); // Not a struct, return as is
    }

    // Set up StackRunner for default handling
    let mut runner = StackRunner {
        original_input: &[],
        input: &[],
        stack: vec![],
        last_span: Span::new(0, 0),
    };

    // Capture shape before moving wip
    let shape = wip.shape();

    // Apply defaults using StackRunner
    runner.pop(wip, PopReason::TopLevel).map_err(|_e| {
        ArgsError::new(ArgsErrorKind::GenericReflect(
            ReflectError::OperationFailed {
                shape,
                operation: "applying defaults",
            },
        ))
    })
}
