use facet_core::{Facet, PointerType, Type};

#[test]
fn shape_name_string_slice_const_ptr() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let shape = <*const str as Facet>::SHAPE;
    match shape.ty {
        Type::Pointer(PointerType::Raw(vpt)) => {
            assert!(!vpt.mutable);
            assert_eq!((vpt.target)().to_string(), "str");
        }
        _ => panic!("wrong type {:?}", shape.ty),
    }

    Ok(())
}

#[test]
fn shape_name_string_slice_mut_ptr() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let shape = <*mut str as Facet>::SHAPE;
    match shape.ty {
        Type::Pointer(PointerType::Raw(vpt)) => {
            assert!(vpt.mutable);
            assert_eq!((vpt.target)().to_string(), "str");
        }
        _ => panic!("wrong type {:?}", shape.ty),
    }

    Ok(())
}

#[test]
fn shape_name_string_slice_ref() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let shape = <&str as Facet>::SHAPE;
    match shape.ty {
        Type::Pointer(PointerType::Reference(vpt)) => {
            assert!(!vpt.mutable);
            assert_eq!((vpt.target)().to_string(), "str");
        }
        _ => panic!("wrong type {:?}", shape.ty),
    }

    Ok(())
}
