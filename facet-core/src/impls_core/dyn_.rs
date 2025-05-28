use crate::{
    DebugFn, DebugFnWide, Facet, PtrConst, PtrConstWide, Shape, Type, UserType, ValueVTable,
};

struct DebugFnCurried<'mem> {
    ptr: PtrConst<'mem>,
    f: DebugFn,
}

impl<'mem> DebugFnCurried<'mem> {
    unsafe fn call(self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        unsafe { (self.f)(self.ptr, f) }
    }
}

pub unsafe trait DynFacet<'a> {
    fn debug(&self) -> Option<DebugFnCurried>;
}

unsafe impl<'a, T: Facet<'a>> DynFacet<'a> for T {
    fn debug(&self) -> Option<DebugFnCurried> {
        let debug = (T::VTABLE.sized().unwrap().debug)()?;
        Some(DebugFnCurried {
            ptr: PtrConst::new(self),
            f: debug,
        })
    }
}

unsafe impl<'a> Facet<'a> for dyn DynFacet<'a> + 'a {
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder_unsized::<Self>()
            .type_name(|f, _opts| write!(f, "dyn DynFacet"))
            .debug(|| {
                Some(|v, f| {
                    if let Some(debug) = v.debug() {
                        unsafe { debug.call(f) }
                    } else {
                        write!(f, "<No Debug impl>")
                    }
                })
            })
            .build()
    };

    const SHAPE: &'static Shape<'static> = &Shape::builder_for_unsized::<Self>()
        .ty(Type::User(UserType::Opaque))
        .type_identifier("dyn DynFacet")
        .build();
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{DebugFnTyped, VTableView};

    fn debug_str<'a, T: Facet<'a> + ?Sized>(v: &T) -> Option<String> {
        let view = VTableView::<T>::of();
        let debug = view.debug()?;

        struct Debugger<'a, T: ?Sized>(&'a T, DebugFnTyped<T>);
        impl<'a, T: ?Sized> core::fmt::Debug for Debugger<'a, T> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                (self.1)(self.0, f)
            }
        }

        Some(format!("{:?}", Debugger(v, debug)))
    }

    #[test]
    fn test_dyn() {
        let s = String::from("abc");
        let s_dyn = &s as &dyn DynFacet;

        assert_eq!(debug_str(s_dyn).as_deref(), Some(r#""abc""#),);

        let vec = vec![1, 2, 3];

        let slice: &[&dyn DynFacet] = &[s_dyn, &vec as &dyn DynFacet, &10 as &dyn DynFacet];
        assert_eq!(
            debug_str(slice).as_deref(),
            Some(r#"["abc", [1, 2, 3], 10]"#),
        );

        let arr: [&dyn DynFacet; 3] = [s_dyn, &vec as &dyn DynFacet, &10 as &dyn DynFacet];
        let arr_dyn: &dyn DynFacet = &arr as &dyn DynFacet;
        assert_eq!(
            debug_str(arr_dyn).as_deref(),
            Some(r#"["abc", [1, 2, 3], 10]"#),
        );
    }
}
