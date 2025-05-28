use std::sync::Arc;

use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::test;

#[derive(Debug, PartialEq, Facet)]
struct Inner {
    value: i32,
}

#[derive(Debug, PartialEq, Facet)]
struct OuterYesArc {
    inner: Arc<Inner>,
}

#[derive(Debug, PartialEq, Facet)]
struct OuterNoArc {
    inner: Inner,
}

#[test]
fn outer_no_arc() {
    let mut partial = Partial::alloc::<OuterNoArc>()?;
    partial.begin_field("inner")?;
    partial.begin_field("value")?;
    partial.set(1234_i32)?;
    partial.end()?;
    partial.end()?;
    let o: Box<OuterNoArc> = partial.build()?;
    assert_eq!(
        *o,
        OuterNoArc {
            inner: Inner { value: 1234 }
        }
    );
}

#[test]
fn outer_yes_arc_put() {
    let mut partial = Partial::alloc::<OuterYesArc>()?;
    let inner = Arc::new(Inner { value: 5678 });
    partial.begin_field("inner")?;
    partial.set(inner.clone())?;
    partial.end()?;
    let o: Box<OuterYesArc> = partial.build()?;
    assert_eq!(*o, OuterYesArc { inner });
}

#[test]
fn outer_yes_arc_pointee() {
    let mut partial = Partial::alloc::<OuterYesArc>()?;
    partial.begin_field("inner")?;
    partial.begin_smart_ptr()?;
    partial.begin_field("value")?;
    partial.set(4321_i32)?;
    partial.end()?;
    partial.end()?;
    partial.end()?;
    let o: Box<OuterYesArc> = partial.build()?;
    assert_eq!(
        *o,
        OuterYesArc {
            inner: Arc::new(Inner { value: 4321 })
        }
    );
}

#[test]
fn outer_yes_arc_field_named_twice_error() {
    let mut partial = Partial::alloc::<OuterYesArc>().unwrap();
    partial.begin_field("inner").unwrap();
    // Try to do begin_field again instead of begin_smart_ptr; this should error
    let err = partial.begin_field("value").err().unwrap();
    let err_string = format!("{err}");
    assert!(
        err_string.contains("opaque types cannot be reflected upon"),
        "Error message should mention 'opaque types cannot be reflected upon', got: {err_string}"
    );
}
