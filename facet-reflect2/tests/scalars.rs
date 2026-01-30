use facet_core::Facet;
use facet_reflect2::{Move, Op, Partial, Path, Source};

#[test]
fn set_u32() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let value = 42u32;
    partial
        .apply(&[Op::Set {
            path: Path::default(),
            source: Source::Move(Move {
                ptr: facet_core::PtrConst::new(&value),
                shape: <u32 as Facet>::SHAPE,
            }),
        }])
        .unwrap();

    let result: u32 = partial.build().unwrap();
    assert_eq!(result, 42);
}
