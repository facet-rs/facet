use std::sync::Arc;

use facet::Facet;
use facet_reflect::Wip;
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
    let wip = Wip::alloc::<OuterNoArc>()?;
    let wip = wip.field_named("inner")?;
    let wip = wip.field_named("value")?;
    let wip = wip.put(1234_i32)?;
    let wip = wip.pop()?;
    let wip = wip.pop()?;
    let wip = wip.build()?;
    let o: OuterNoArc = wip.materialize()?;
    assert_eq!(
        o,
        OuterNoArc {
            inner: Inner { value: 1234 }
        }
    );
}

#[test]
fn outer_yes_arc_put() {
    let wip = Wip::alloc::<OuterYesArc>()?;
    let inner = Arc::new(Inner { value: 5678 });
    let wip = wip.field_named("inner")?;
    let wip = wip.put(inner.clone())?;
    let wip = wip.pop()?;
    let wip = wip.build()?;
    let o: OuterYesArc = wip.materialize()?;
    assert_eq!(o, OuterYesArc { inner });
}

#[test]
fn outer_yes_arc_pointee() {
    let wip = Wip::alloc::<OuterYesArc>()?;
    let wip = wip.field_named("inner")?;
    let wip = wip.push_pointee()?;
    let wip = wip.field_named("value")?;
    let wip = wip.put(4321_i32)?;
    let wip = wip.pop()?;
    let wip = wip.pop()?;
    let wip = wip.pop()?;
    let wip = wip.build()?;
    let o: OuterYesArc = wip.materialize()?;
    assert_eq!(
        o,
        OuterYesArc {
            inner: Arc::new(Inner { value: 4321 })
        }
    );
}

#[test]
fn outer_yes_arc_field_named_twice_error() {
    let wip = Wip::alloc::<OuterYesArc>().unwrap();
    let wip = wip.field_named("inner").unwrap();
    // Try to do field_named again instead of push_pointee; this should error
    let err = wip.field_named("value").err().unwrap();
    let err_string = format!("{err}");
    assert!(
        err_string.contains("push_pointee"),
        "Error message should mention 'push_pointee', got: {err_string}"
    );
}
