// =============================================================================
// Immediate Mode
// =============================================================================

/// Immediate mode partial - validates on End, no frame storage
pub(crate) struct ImmediatePartial<'facet, const BORROW: bool> {
    arena: FrameArena, // FIXME: just use Arena<T> from arena.rs
    stack: Vec<FrameId>,
    root_plan: std::sync::Arc<crate::typeplan::TypePlanCore>,
    state: PartialState,
    _marker: std::marker::PhantomData<&'facet ()>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PartialState {
    Active,
    Built,
    Poisoned,
}

impl<'facet, const BORROW: bool> ImmediatePartial<'facet, BORROW> {
    /// Process a batch of operations (no BeginDeferred/FinishDeferred)
    fn submit(&mut self, ops: &[PartialOp<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                // Scalars
                PartialOp::Set { .. } => todo!(),

                // Structs
                PartialOp::BeginField { .. } => todo!(),
                PartialOp::BeginNthField { .. } => todo!(),

                // Enums
                PartialOp::SelectVariant { .. } => todo!(),
                PartialOp::SelectNthVariant { .. } => todo!(),

                // Options
                PartialOp::BeginSome => todo!(),
                PartialOp::SetNone => todo!(),

                // Results
                PartialOp::BeginOk => todo!(),
                PartialOp::BeginErr => todo!(),

                // Lists
                PartialOp::InitList => todo!(),
                PartialOp::BeginListItem => todo!(),

                // Arrays
                PartialOp::InitArray => todo!(),

                // Maps
                PartialOp::InitMap => todo!(),
                PartialOp::BeginKey => todo!(),
                PartialOp::BeginValue => todo!(),

                // Sets
                PartialOp::InitSet => todo!(),
                PartialOp::BeginSetItem => todo!(),

                // Smart pointers
                PartialOp::BeginSmartPtr => todo!(),
                PartialOp::BeginInner => todo!(),

                // Defaults
                PartialOp::SetDefault => todo!(),
                PartialOp::SetNthFieldToDefault { .. } => todo!(),

                // Parsing
                PartialOp::ParseFromStr { .. } => todo!(),

                // Navigation
                PartialOp::End => todo!(),

                // Mode switches (handled by Partial wrapper)
                PartialOp::BeginDeferred | PartialOp::FinishDeferred => {
                    unreachable!("mode switches handled by Partial::submit")
                }
            }
        }
        Ok(())
    }
}
