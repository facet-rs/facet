//! ReprAffinity: semantic hints that structure alone cannot express.

use facet_core::{Facet, ReprAffinity};

#[test]
fn shape_stays_pointer_bucket_sized() {
    // The affinity field must ride in Shape's repr(C) padding after `flags`.
    // If this grows, the field got moved or the padding got consumed —
    // reconsider placement before accepting the size increase.
    #[cfg(target_pointer_width = "64")]
    assert_eq!(core::mem::size_of::<facet_core::Shape>(), 480);
}

#[test]
fn default_affinity_is_none() {
    assert_eq!(<Vec<u8> as Facet>::SHAPE.affinity, ReprAffinity::None);
    assert_eq!(<String as Facet>::SHAPE.affinity, ReprAffinity::None);
    assert_eq!(u8::SHAPE.affinity, ReprAffinity::None);
}

#[cfg(feature = "bstr")]
#[test]
fn bstr_shapes_have_byte_string_affinity() {
    use bstr::{BStr, BString};
    assert_eq!(<BString as Facet>::SHAPE.affinity, ReprAffinity::ByteString);
    assert_eq!(<BStr as Facet>::SHAPE.affinity, ReprAffinity::ByteString);
    // affinity is a hint, not a structural change: still a List<u8> mechanically
    assert!(matches!(
        <BString as Facet>::SHAPE.def,
        facet_core::Def::List(_)
    ));
}
