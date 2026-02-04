use std::sync::Arc;

use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::{IPanic, test};

#[derive(Facet, Debug, PartialEq)]
struct Inner {
    x: u32,
    y: String,
}

/// Test SmartPointer with deferred inner frame storage.
///
/// This reproduces the core issue from #2020: SmartPointer's inner frame should be
/// stored for deferred processing when end() is called in deferred mode.
/// The inner frame pointer needs to be saved (like Option does with pending_inner)
/// so it can be finalized in finish_deferred().
///
/// The bug: The workaround prevents the inner frame from being stored at all.
/// The fix: SmartPointer should track pending_inner like Option does.
#[test]
fn smartptr_inner_frame_deferred() -> Result<(), IPanic> {
    let mut partial = Partial::alloc::<Arc<Inner>>()?;

    // Enable deferred mode - this triggers storage of frames when end() is called
    partial = partial.begin_deferred()?;

    // Begin building the smart pointer's inner value
    partial = partial.begin_smart_ptr()?;

    // Set inner fields
    partial = partial.set_field("x", 42u32)?;
    partial = partial.set_field("y", String::from("hello"))?;

    // Pop the inner frame - in deferred mode, this should store the inner frame
    // pointer in SmartPointer's pending_inner, not prevent storage via workaround
    partial = partial.end()?; // end smart ptr's inner value

    // Finalize deferred - should process stored inner frame and complete SmartPointer
    partial = partial.finish_deferred()?;

    // Build and verify
    let arc = partial.build()?.materialize::<Arc<Inner>>()?;
    assert_eq!(arc.x, 42);
    assert_eq!(arc.y, "hello");

    Ok(())
}
