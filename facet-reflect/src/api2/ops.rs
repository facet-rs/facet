// =============================================================================
// Operations
// =============================================================================

use facet_core::Shape;

/// An operation to execute on a Partial.
///
/// Operations are processed in batches via `submit()`. Pointers in `Set` ops
/// are only valid during the `submit()` call - data is copied immediately.
#[derive(Clone, Copy)]
pub enum PartialOp<'a> {
    // -------------------------------------------------------------------------
    // Scalars
    // -------------------------------------------------------------------------
    /// Set the current value (type-erased pointer + shape)
    Set {
        ptr: *const (),
        shape: &'static Shape,
    },

    // -------------------------------------------------------------------------
    // Structs
    // -------------------------------------------------------------------------
    /// Begin a struct field by name
    BeginField { name: &'a str },

    /// Begin a struct field by index
    BeginNthField { index: usize },

    // -------------------------------------------------------------------------
    // Enums
    // -------------------------------------------------------------------------
    /// Select an enum variant by name
    SelectVariant { name: &'a str },

    /// Select an enum variant by index
    SelectNthVariant { index: usize },

    // -------------------------------------------------------------------------
    // Options
    // -------------------------------------------------------------------------
    /// Begin the Some variant of an Option
    BeginSome,

    /// Set an Option to None
    SetNone,

    // -------------------------------------------------------------------------
    // Results
    // -------------------------------------------------------------------------
    /// Begin the Ok variant of a Result
    BeginOk,

    /// Begin the Err variant of a Result
    BeginErr,

    // -------------------------------------------------------------------------
    // Lists (Vec, etc.)
    // -------------------------------------------------------------------------
    /// Initialize a list
    InitList,

    /// Begin a list item
    BeginListItem,

    // -------------------------------------------------------------------------
    // Arrays
    // -------------------------------------------------------------------------
    /// Initialize an array
    InitArray,

    // -------------------------------------------------------------------------
    // Maps (HashMap, etc.)
    // -------------------------------------------------------------------------
    /// Initialize a map
    InitMap,

    /// Begin a map key
    BeginKey,

    /// Begin a map value
    BeginValue,

    // -------------------------------------------------------------------------
    // Sets (HashSet, etc.)
    // -------------------------------------------------------------------------
    /// Initialize a set
    InitSet,

    /// Begin a set item
    BeginSetItem,

    // -------------------------------------------------------------------------
    // Smart pointers (Box, Arc, Rc)
    // -------------------------------------------------------------------------
    /// Begin smart pointer inner value
    BeginSmartPtr,

    /// Begin transparent inner (newtype wrappers)
    BeginInner,

    // -------------------------------------------------------------------------
    // Defaults
    // -------------------------------------------------------------------------
    /// Set current value to its default
    SetDefault,

    /// Set nth field to its default value
    SetNthFieldToDefault { index: usize },

    // -------------------------------------------------------------------------
    // Parsing
    // -------------------------------------------------------------------------
    /// Parse from string (for FromStr types)
    ParseFromStr { s: &'a str },

    // -------------------------------------------------------------------------
    // Navigation
    // -------------------------------------------------------------------------
    /// End the current frame, return to parent
    End,

    // -------------------------------------------------------------------------
    // Mode switches
    // -------------------------------------------------------------------------
    /// Enter deferred mode
    BeginDeferred,

    /// Exit deferred mode, validate everything
    FinishDeferred,
}
