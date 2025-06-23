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

#[test]
fn arc_str_begin_smart_ptr_good() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.set(String::from("foobar"))?;
    partial.end()?;
    let built = *partial.build()?;
    assert_eq!(&*built, "foobar");
}

#[test]
fn arc_str_begin_smart_ptr_bad_1() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    let _err = partial.build().unwrap_err();
    #[cfg(not(miri))]
    insta::assert_snapshot!(_err);
}

#[test]
fn arc_str_begin_smart_ptr_bad_2() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    partial.begin_smart_ptr()?;
    let _err = partial.build().unwrap_err();
    #[cfg(not(miri))]
    insta::assert_snapshot!(_err);
}

#[test]
fn arc_str_begin_smart_ptr_bad_3() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.set(String::from("foobar"))?;
    let _err = partial.build().unwrap_err();
    #[cfg(not(miri))]
    insta::assert_snapshot!(_err);
}

#[test]
fn rc_str_begin_smart_ptr() {
    use std::rc::Rc;
    let mut partial = Partial::alloc::<Rc<str>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.set(String::from("foobar"))?;
    partial.end()?;
    let built = *partial.build()?;
    assert_eq!(&*built, "foobar");
}

#[test]
fn box_str_begin_smart_ptr() {
    let mut partial = Partial::alloc::<Box<str>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.set(String::from("foobar"))?;
    partial.end()?;
    let built = *partial.build()?;
    assert_eq!(&*built, "foobar");
}

#[test]
fn arc_slice_u8_begin_smart_ptr_good() {
    {
        // Just to make sure: Vec<u8> construction works
        let mut partial = Partial::alloc::<Vec<u8>>().unwrap();
        partial.begin_list()?;
        partial.push(2_u8)?;
        partial.push(3_u8)?;
        partial.push(4_u8)?;
        let built = *partial.build()?;
        assert_eq!(&*built, &[2, 3, 4]);
    }

    {
        // Now, does Arc<[u8]> work?
        let mut partial = Partial::alloc::<Arc<[u8]>>().unwrap();
        partial.begin_smart_ptr()?;
        partial.begin_list()?;
        partial.push(2_u8)?;
        partial.push(3_u8)?;
        partial.push(4_u8)?;
        partial.end()?;
        let built = *partial.build()?;
        assert_eq!(&*built, &[2, 3, 4]);
    }
}
