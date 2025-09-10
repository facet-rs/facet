use std::sync::Arc;

use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::{IPanic, test};

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
    let mut partial = Partial::alloc::<OuterNoArc>().unwrap();
    partial.begin_field("inner").unwrap();
    partial.begin_field("value").unwrap();
    partial.set(1234_i32).unwrap();
    partial.end().unwrap();
    partial.end().unwrap();
    let o: Box<OuterNoArc> = partial.build().unwrap();
    assert_eq!(
        *o,
        OuterNoArc {
            inner: Inner { value: 1234 }
        }
    );
}

#[test]
fn outer_yes_arc_put() {
    let mut partial = Partial::alloc::<OuterYesArc>().unwrap();
    let inner = Arc::new(Inner { value: 5678 });
    partial.begin_field("inner").unwrap();
    partial.set(inner.clone()).unwrap();
    partial.end().unwrap();
    let o: Box<OuterYesArc> = partial.build().unwrap();
    assert_eq!(*o, OuterYesArc { inner });
}

#[test]
fn outer_yes_arc_pointee() {
    let mut partial = Partial::alloc::<OuterYesArc>().unwrap();
    partial.begin_field("inner").unwrap();
    partial.begin_smart_ptr().unwrap();
    partial.begin_field("value").unwrap();
    partial.set(4321_i32).unwrap();
    partial.end().unwrap();
    partial.end().unwrap();
    partial.end().unwrap();
    let o: Box<OuterYesArc> = partial.build().unwrap();
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
    partial.begin_smart_ptr().unwrap();
    partial.set(String::from("foobar")).unwrap();
    partial.end().unwrap();
    let built = *partial.build().unwrap();
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
fn arc_str_begin_smart_ptr_bad_2a() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    partial.begin_smart_ptr().unwrap();
    let _err = partial.build().unwrap_err();
    #[cfg(not(miri))]
    insta::assert_snapshot!(_err);
}

#[test]
fn arc_str_begin_smart_ptr_bad_2b() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    partial.begin_smart_ptr().unwrap();
    let _err = partial.end().unwrap_err();
    #[cfg(not(miri))]
    insta::assert_snapshot!(_err);
}

#[test]
fn arc_str_begin_smart_ptr_bad_3() {
    let mut partial = Partial::alloc::<Arc<str>>().unwrap();
    partial.begin_smart_ptr().unwrap();
    partial.set(String::from("foobar")).unwrap();
    let _err = partial.build().unwrap_err();
    #[cfg(not(miri))]
    insta::assert_snapshot!(_err);
}

#[test]
fn rc_str_begin_smart_ptr_once() {
    use std::rc::Rc;
    let mut partial = Partial::alloc::<Rc<str>>().unwrap();
    partial.begin_smart_ptr().unwrap();
    partial.set(String::from("foobar")).unwrap();
    partial.end().unwrap();
    let built = *partial.build().unwrap();
    assert_eq!(&*built, "foobar");
}

#[test]
fn rc_str_begin_smart_ptr_twice() -> Result<(), IPanic> {
    use std::rc::Rc;
    let mut partial = Partial::alloc::<Rc<str>>()?;

    eprintln!("==== first go");
    partial.begin_smart_ptr()?;
    partial.set(String::from("foobar"))?;
    partial.end()?;

    eprintln!("==== second go");
    partial.begin_smart_ptr()?;
    partial.set(String::from("barbaz"))?;
    partial.end()?;

    eprintln!("==== build");
    let built = *partial.build()?;
    assert_eq!(&*built, "barbaz");

    Ok(())
}

#[test]
fn box_str_begin_smart_ptr() {
    let mut partial = Partial::alloc::<Box<str>>().unwrap();
    partial.begin_smart_ptr().unwrap();
    partial.set(String::from("foobar")).unwrap();
    partial.end().unwrap();
    let built = *partial.build().unwrap();
    assert_eq!(&*built, "foobar");
}

#[test]
fn arc_slice_u8_begin_smart_ptr_good() {
    {
        // Just to make sure: Vec<u8> construction works
        let mut partial = Partial::alloc::<Vec<u8>>().unwrap();
        partial.begin_list().unwrap();
        partial.push(2_u8).unwrap();
        partial.push(3_u8).unwrap();
        partial.push(4_u8).unwrap();
        let built = *partial.build().unwrap();
        assert_eq!(&*built, &[2, 3, 4]);
    }

    {
        // Now, does Arc<[u8]> work.unwrap()
        let mut partial = Partial::alloc::<Arc<[u8]>>().unwrap();
        partial.begin_smart_ptr().unwrap();
        partial.begin_list().unwrap();
        partial.push(2_u8).unwrap();
        partial.push(3_u8).unwrap();
        partial.push(4_u8).unwrap();
        partial.end().unwrap();
        let built = *partial.build().unwrap();
        assert_eq!(&*built, &[2, 3, 4]);
    }
}
