use facet_core::{Facet, PointerType, Type};
use facet_testhelpers::test;

#[test]
fn shape_name_string_slice_const_ptr() {
    let shape = <*const str as Facet>::SHAPE;
    match shape.ty {
        Type::Pointer(PointerType::Raw(vpt)) => {
            assert!(!vpt.mutable);
            assert_eq!((vpt.target)().to_string(), "str");
        }
        _ => panic!("wrong type {:?}", shape.ty),
    }
}

#[test]
fn shape_name_string_slice_mut_ptr() {
    let shape = <*mut str as Facet>::SHAPE;
    match shape.ty {
        Type::Pointer(PointerType::Raw(vpt)) => {
            assert!(vpt.mutable);
            assert_eq!((vpt.target)().to_string(), "str");
        }
        _ => panic!("wrong type {:?}", shape.ty),
    }
}

#[test]
fn shape_name_string_slice_ref() {
    let shape = <&str as Facet>::SHAPE;
    match shape.ty {
        Type::Pointer(PointerType::Reference(vpt)) => {
            assert!(!vpt.mutable);
            assert_eq!((vpt.target)().to_string(), "str");
        }
        _ => panic!("wrong type {:?}", shape.ty),
    }
}
