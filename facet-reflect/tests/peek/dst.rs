use facet_reflect::Peek;

// miri is NOT happy with PtrConstWide, cf.
//     error: Undefined Behavior: calling a function with argument of type &str passing data of type facet::PtrConstWide<'_>
//    --> /__w/facet/facet/facet-reflect/src/peek/value.rs:520:37
//     |
// 520 |                     return unsafe { display_fn(ptr, f) };
//     |                                     ^^^^^^^^^^^^^^^^^^ calling a function with argument of type &str passing data of type facet::PtrConstWide<'_>
//     |
//     = help: this indicates a bug in the program: it performed an invalid operation, and caused Undefined Behavior
//     = help: see https://doc.rust-lang.org/nightly/reference/behavior-considered-undefined.html for further information
//     = help: this means these two types are not *guaranteed* to be ABI-compatible across all targets
//     = help: if you think this code should be accepted anyway, please report an issue with Miri
#[test]
#[cfg(not(miri))]
fn test_peek_dst_str() {
    let s = "abc";
    let peek = Peek::new(s);
    assert_eq!(format!("{peek}"), format!("{s}"));
    assert_eq!(format!("{peek:?}"), format!("{s:?}"));
}

#[test]
#[cfg(not(miri))]
fn test_peek_dst_slice() {
    let s = [1, 2, 3].as_slice();
    let peek = Peek::new(s);
    assert_eq!(format!("{peek:?}"), format!("{s:?}"));
}
