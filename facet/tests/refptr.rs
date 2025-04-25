use facet::{DebugFn, Facet, PtrConst};
use std::fmt::Debug;

fn vtable_debug<'a, T: Facet<'a> + ?Sized>(v: &T) -> Option<String> {
    struct VTableDebug<'a, T: ?Sized>(&'a T, DebugFn);

    impl<T: ?Sized> Debug for VTableDebug<'_, T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unsafe { self.1(PtrConst::new(self.0), f) }
        }
    }

    T::SHAPE
        .vtable
        .debug
        .map(|debug| format!("{:?}", VTableDebug(v, debug)))
}

#[test]
fn test_fields() {
    #[allow(dead_code)]
    #[derive(Debug, Facet)]
    struct S<'a> {
        static_ref: &'static String,
        a_ref: &'a String,
        static_mut: &'static mut String,
        a_mut: &'a mut String,
        ptr: *const String,
        mut_ptr: *mut String,
    }
}

#[test]
fn test_ref() {
    let s = String::from("abc");
    let s = &s;
    assert_eq!(vtable_debug::<&String>(&s), Some(format!("{s:?}")));
}

#[test]
fn test_mut_ref() {
    let mut s = String::from("abc");
    let s = &mut s;
    assert_eq!(vtable_debug::<&mut String>(&s), Some(format!("{s:?}")));
}

#[test]
fn test_ptr() {
    let s = String::from("abc");
    let s = &raw const s;
    assert_eq!(vtable_debug::<*const String>(&s), Some(format!("{s:?}")));
}

#[test]
fn test_mut_ptr() {
    let mut s = String::from("abc");
    let s = &raw mut s;
    assert_eq!(vtable_debug::<*mut String>(&s), Some(format!("{s:?}")));
}

#[test]
fn test_u8_debug() {
    let s = 1u8;
    assert_eq!(vtable_debug::<u8>(&s), Some(format!("{s:?}")));
}

#[test]
fn test_slice_debug() {
    let s = &[1, 2, 3][..];
    assert_eq!(vtable_debug::<[u8]>(s), Some(format!("{s:?}")));
}

#[test]
fn test_slice_ref_debug() {
    let s = &[1, 2, 3][..];
    assert_eq!(vtable_debug::<&[u8]>(&s), Some(format!("{s:?}")));
}
