use core::hash::Hash;

use crate::{ConstTypeId, Def, Facet, HasherProxy, MarkerTraits, PtrConst, Shape, ValueVTable};

fn get_str(value: PtrConst<'_>) -> &str {
    let len = unsafe { value.fat_part().unwrap_unchecked() };
    let ptr = unsafe { value.as_ptr::<u8>() };
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    unsafe { core::str::from_utf8_unchecked(slice) }
}

unsafe impl Facet<'_> for str {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .set_unsized()
            .def(Def::Str)
            .vtable(
                &const {
                    ValueVTable::builder()
                        .type_name(|f, _opts| write!(f, "str"))
                        .marker_traits(MarkerTraits::EQ)
                        .display(|value, f| {
                            let s = get_str(value);
                            write!(f, "{s}")
                        })
                        .debug(|value, f| {
                            let s = get_str(value);
                            write!(f, "{s:?}")
                        })
                        .eq(|a, b| get_str(a) == get_str(b))
                        .ord(|a, b| get_str(a).cmp(get_str(b)))
                        .partial_ord(|a, b| get_str(a).partial_cmp(get_str(b)))
                        .hash(|value, state, hasher| {
                            let s = get_str(value);
                            let proxy = &mut unsafe { HasherProxy::new(state, hasher) };
                            s.hash(proxy)
                        })
                        .build()
                },
            )
            .build()
    };
}
