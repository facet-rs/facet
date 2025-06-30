use alloc::boxed::Box;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

use crate::{
    Def, Facet, KnownSmartPointer, PtrConst, PtrConstWide, PtrMut, PtrUninit, Shape,
    SliceBuilderVTable, SmartPointerDef, SmartPointerFlags, SmartPointerVTable,
    TryBorrowInnerError, TryFromError, TryIntoInnerError, Type, UserType, ValueVTable,
    value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Arc<T> {
    const VTABLE: &'static ValueVTable = &const {
        // Define the functions for transparent conversion between Arc<T> and T
        unsafe fn try_from<'a, 'shape, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            src_shape: &'shape Shape<'shape>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
            if src_shape.id != T::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[T::SHAPE],
                });
            }
            let t = unsafe { src_ptr.read::<T>() };
            let arc = Arc::new(t);
            Ok(unsafe { dst.put(arc) })
        }

        unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrMut<'src>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            use alloc::sync::Arc;

            // Read the Arc from the source pointer
            let arc = unsafe { src_ptr.read::<Arc<T>>() };

            // Try to unwrap the Arc to get exclusive ownership
            match Arc::try_unwrap(arc) {
                Ok(inner) => Ok(unsafe { dst.put(inner) }),
                Err(arc) => {
                    // Arc is shared, so we can't extract the inner value
                    core::mem::forget(arc);
                    Err(TryIntoInnerError::Unavailable)
                }
            }
        }

        unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let arc = unsafe { src_ptr.get::<Arc<T>>() };
            Ok(PtrConst::new(&**arc))
        }

        let mut vtable = value_vtable!(alloc::sync::Arc<T>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (T::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        });

        {
            let vtable = vtable.sized_mut().unwrap();
            vtable.try_from = || Some(try_from::<T>);
            vtable.try_into_inner = || Some(try_into_inner::<T>);
            vtable.try_borrow_inner = || Some(try_borrow_inner::<T>);
        }
        vtable
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape<'static> {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("Arc")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| T::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC)
                    .known(KnownSmartPointer::Arc)
                    .weak(|| <Weak<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let arc_ptr = unsafe { this.as_ptr::<Arc<T>>() };
                                    let ptr = unsafe { Arc::as_ptr(&*arc_ptr) };
                                    PtrConst::new(ptr).into()
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let arc = Arc::new(t);
                                    unsafe { this.put(arc) }
                                })
                                .downgrade_into_fn(|strong, weak| unsafe {
                                    weak.put(Arc::downgrade(strong.get::<Self>()))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a> Facet<'a> for Arc<str> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::sync::Arc<str>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (str::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        })
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape<'static> {
            str::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("Arc")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || str::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| str::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC)
                    .known(KnownSmartPointer::Arc)
                    .weak(|| <Weak<str> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| unsafe {
                                    let concrete = this.get::<Arc<str>>();
                                    let s: &str = concrete;
                                    PtrConstWide::new(&raw const *s).into()
                                })
                                .new_into_fn(|_this, _ptr| todo!())
                                .downgrade_into_fn(|_strong, _weak| todo!())
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Arc<[U]> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::sync::Arc<[U]>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (<[U]>::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        })
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, U: Facet<'a>>() -> &'static Shape<'static> {
            <[U]>::SHAPE
        }

        fn slice_builder_new<'a, U: Facet<'a>>() -> PtrMut<'static> {
            let v = Box::new(Vec::<U>::new());
            let raw = Box::into_raw(v);
            PtrMut::new(raw)
        }

        fn slice_builder_push<'a, U: Facet<'a>>(builder: PtrMut, item: PtrMut) {
            unsafe {
                let vec = builder.as_mut::<Vec<U>>();
                let value = item.read::<U>();
                vec.push(value);
            }
        }

        fn slice_builder_convert<'a, U: Facet<'a>>(builder: PtrMut<'static>) -> PtrConst<'static> {
            unsafe {
                let vec_box = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
                let arc: Arc<[U]> = (*vec_box).into();
                let arc_box = Box::new(arc);
                PtrConst::new(Box::into_raw(arc_box) as *const Arc<[U]>)
            }
        }

        fn slice_builder_free<'a, U: Facet<'a>>(builder: PtrMut<'static>) {
            unsafe {
                let _ = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
            }
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("Arc")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || <[U]>::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| <[U]>::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC)
                    .known(KnownSmartPointer::Arc)
                    .weak(|| <Weak<[U]> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| unsafe {
                                    let concrete = this.get::<Arc<[U]>>();
                                    let s: &[U] = concrete;
                                    PtrConstWide::new(&raw const *s).into()
                                })
                                .new_into_fn(|_this, _ptr| todo!())
                                .downgrade_into_fn(|_strong, _weak| todo!())
                                .slice_builder_vtable(
                                    &const {
                                        SliceBuilderVTable::builder()
                                            .new_fn(slice_builder_new::<U>)
                                            .push_fn(slice_builder_push::<U>)
                                            .convert_fn(slice_builder_convert::<U>)
                                            .free_fn(slice_builder_free::<U>)
                                            .build()
                                    },
                                )
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<U>)
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Weak<T> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::sync::Weak<T>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (T::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        })
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape<'static> {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("Weak")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| T::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC.union(SmartPointerFlags::WEAK))
                    .known(KnownSmartPointer::ArcWeak)
                    .strong(|| <Arc<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .upgrade_into_fn(|weak, strong| unsafe {
                                    Some(strong.put(weak.get::<Self>().upgrade()?))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a> Facet<'a> for Weak<str> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::sync::Weak<str>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (str::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        })
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape<'static> {
            str::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("Weak")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || str::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| str::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC.union(SmartPointerFlags::WEAK))
                    .known(KnownSmartPointer::ArcWeak)
                    .strong(|| <Arc<str> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .upgrade_into_fn(|_weak, _strong| todo!())
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Weak<[U]> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::sync::Weak<[U]>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (<[U]>::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        })
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        fn inner_shape<'a, U: Facet<'a>>() -> &'static Shape<'static> {
            <[U]>::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("Weak")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || <[U]>::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| <[U]>::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC.union(SmartPointerFlags::WEAK))
                    .known(KnownSmartPointer::ArcWeak)
                    .strong(|| <Arc<[U]> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .upgrade_into_fn(|weak, strong| unsafe {
                                    Some(strong.put(weak.get::<Self>().upgrade()?))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<U>)
            .build()
    };
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::sync::{Arc, Weak as ArcWeak};

    use super::*;

    #[test]
    fn test_arc_type_params() {
        let [type_param_1] = <Arc<i32>>::SHAPE.type_params else {
            panic!("Arc<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_arc_vtable_1_new_borrow_drop() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_smart_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        // Allocate memory for the Arc
        let arc_uninit_ptr = arc_shape.allocate()?;

        // Get the function pointer for creating a new Arc from a value
        let new_into_fn = arc_def
            .vtable
            .new_into_fn
            .expect("Arc<T> should have new_into_fn");

        // Create the value and initialize the Arc
        let mut value = String::from("example");
        let arc_ptr = unsafe { new_into_fn(arc_uninit_ptr, PtrMut::new(&raw mut value)) };
        // The value now belongs to the Arc, prevent its drop
        core::mem::forget(value);

        // Get the function pointer for borrowing the inner value
        let borrow_fn = arc_def
            .vtable
            .borrow_fn
            .expect("Arc<T> should have borrow_fn");

        // Borrow the inner value and check it
        let borrowed_ptr = unsafe { borrow_fn(arc_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String within the Arc
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // Get the function pointer for dropping the Arc
        let drop_fn = (arc_shape.vtable.sized().unwrap().drop_in_place)()
            .expect("Arc<T> should have drop_in_place");

        // Drop the Arc in place
        // SAFETY: arc_ptr points to a valid Arc<String>
        unsafe { drop_fn(arc_ptr) };

        // Deallocate the memory
        // SAFETY: arc_ptr was allocated by arc_shape and is now dropped (but memory is still valid)
        unsafe { arc_shape.deallocate_mut(arc_ptr)? };

        Ok(())
    }

    #[test]
    fn test_arc_vtable_2_downgrade_upgrade_drop() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_smart_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        let weak_shape = <ArcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_smart_pointer()
            .expect("ArcWeak<T> should have a smart pointer definition");

        // 1. Create the first Arc (arc1)
        let arc1_uninit_ptr = arc_shape.allocate()?;
        let new_into_fn = arc_def.vtable.new_into_fn.unwrap();
        let mut value = String::from("example");
        let arc1_ptr = unsafe { new_into_fn(arc1_uninit_ptr, PtrMut::new(&raw mut value)) };
        core::mem::forget(value); // Value now owned by arc1

        // 2. Downgrade arc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate()?;
        let downgrade_into_fn = arc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: arc1_ptr points to a valid Arc, weak1_uninit_ptr is allocated for a Weak
        let weak1_ptr = unsafe { downgrade_into_fn(arc1_ptr, weak1_uninit_ptr) };

        // 3. Upgrade weak1 to create a second Arc (arc2)
        let arc2_uninit_ptr = arc_shape.allocate()?;
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        // SAFETY: weak1_ptr points to a valid Weak, arc2_uninit_ptr is allocated for an Arc.
        // Upgrade should succeed as arc1 still exists.
        let arc2_ptr = unsafe { upgrade_into_fn(weak1_ptr, arc2_uninit_ptr) }
            .expect("Upgrade should succeed while original Arc exists");

        // Check the content of the upgraded Arc
        let borrow_fn = arc_def.vtable.borrow_fn.unwrap();
        // SAFETY: arc2_ptr points to a valid Arc<String>
        let borrowed_ptr = unsafe { borrow_fn(arc2_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // 4. Drop everything and free memory
        let arc_drop_fn = (arc_shape.vtable.sized().unwrap().drop_in_place)().unwrap();
        let weak_drop_fn = (weak_shape.vtable.sized().unwrap().drop_in_place)().unwrap();

        unsafe {
            // Drop Arcs
            arc_drop_fn(arc1_ptr);
            arc_shape.deallocate_mut(arc1_ptr)?;
            arc_drop_fn(arc2_ptr);
            arc_shape.deallocate_mut(arc2_ptr)?;

            // Drop Weak
            weak_drop_fn(weak1_ptr);
            weak_shape.deallocate_mut(weak1_ptr)?;
        }

        Ok(())
    }

    #[test]
    fn test_arc_vtable_3_downgrade_drop_try_upgrade() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_smart_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        let weak_shape = <ArcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_smart_pointer()
            .expect("ArcWeak<T> should have a smart pointer definition");

        // 1. Create the strong Arc (arc1)
        let arc1_uninit_ptr = arc_shape.allocate()?;
        let new_into_fn = arc_def.vtable.new_into_fn.unwrap();
        let mut value = String::from("example");
        let arc1_ptr = unsafe { new_into_fn(arc1_uninit_ptr, PtrMut::new(&raw mut value)) };
        core::mem::forget(value);

        // 2. Downgrade arc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate()?;
        let downgrade_into_fn = arc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: arc1_ptr is valid, weak1_uninit_ptr is allocated for Weak
        let weak1_ptr = unsafe { downgrade_into_fn(arc1_ptr, weak1_uninit_ptr) };

        // 3. Drop and free the strong pointer (arc1)
        let arc_drop_fn = (arc_shape.vtable.sized().unwrap().drop_in_place)().unwrap();
        unsafe {
            arc_drop_fn(arc1_ptr);
            arc_shape.deallocate_mut(arc1_ptr)?;
        }

        // 4. Attempt to upgrade the weak pointer (weak1)
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        let arc2_uninit_ptr = arc_shape.allocate()?;
        // SAFETY: weak1_ptr is valid (though points to dropped data), arc2_uninit_ptr is allocated for Arc
        let upgrade_result = unsafe { upgrade_into_fn(weak1_ptr, arc2_uninit_ptr) };

        // Assert that the upgrade failed
        assert!(
            upgrade_result.is_none(),
            "Upgrade should fail after the strong Arc is dropped"
        );

        // 5. Clean up: Deallocate the memory intended for the failed upgrade and drop/deallocate the weak pointer
        let weak_drop_fn = (weak_shape.vtable.sized().unwrap().drop_in_place)().unwrap();
        unsafe {
            // Deallocate the *uninitialized* memory allocated for the failed upgrade attempt
            arc_shape.deallocate_uninit(arc2_uninit_ptr)?;

            // Drop and deallocate the weak pointer
            weak_drop_fn(weak1_ptr);
            weak_shape.deallocate_mut(weak1_ptr)?;
        }

        Ok(())
    }

    #[test]
    fn test_arc_vtable_4_try_from() -> eyre::Result<()> {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let string_shape = <String>::SHAPE;
        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_smart_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        // 1. Create a String value
        let value = String::from("try_from test");
        let value_ptr = PtrConst::new(&value as *const String as *const u8);

        // 2. Allocate memory for the Arc<String>
        let arc_uninit_ptr = arc_shape.allocate()?;

        // 3. Get the try_from function from the Arc<String> shape's ValueVTable
        let try_from_fn =
            (arc_shape.vtable.sized().unwrap().try_from)().expect("Arc<T> should have try_from");

        // 4. Try to convert String to Arc<String>
        let arc_ptr = unsafe { try_from_fn(value_ptr, string_shape, arc_uninit_ptr) }
            .expect("try_from should succeed");
        core::mem::forget(value);

        // 5. Borrow the inner value and verify it's correct
        let borrow_fn = arc_def
            .vtable
            .borrow_fn
            .expect("Arc<T> should have borrow_fn");
        let borrowed_ptr = unsafe { borrow_fn(arc_ptr.as_const()) };

        // SAFETY: borrowed_ptr points to a valid String within the Arc
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "try_from test");

        // 6. Clean up
        let drop_fn = (arc_shape.vtable.sized().unwrap().drop_in_place)()
            .expect("Arc<T> should have drop_in_place");

        unsafe {
            drop_fn(arc_ptr);
            arc_shape.deallocate_mut(arc_ptr)?;
        }

        Ok(())
    }

    #[test]
    fn test_arc_vtable_5_try_into_inner() -> eyre::Result<()> {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let string_shape = <String>::SHAPE;
        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_smart_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        // 1. Create an Arc<String>
        let arc_uninit_ptr = arc_shape.allocate()?;
        let new_into_fn = arc_def
            .vtable
            .new_into_fn
            .expect("Arc<T> should have new_into_fn");

        let mut value = String::from("try_into_inner test");
        let arc_ptr = unsafe { new_into_fn(arc_uninit_ptr, PtrMut::new(&raw mut value)) };
        core::mem::forget(value); // Value now owned by arc

        // 2. Allocate memory for the extracted String
        let string_uninit_ptr = string_shape.allocate()?;

        // 3. Get the try_into_inner function from the Arc<String>'s ValueVTable
        let try_into_inner_fn = (arc_shape.vtable.sized().unwrap().try_into_inner)()
            .expect("Arc<T> Shape should have try_into_inner");

        // 4. Try to extract the String from the Arc<String>
        // This should succeed because we have exclusive access to the Arc (strong count = 1)
        let string_ptr = unsafe { try_into_inner_fn(arc_ptr, string_uninit_ptr) }
            .expect("try_into_inner should succeed with exclusive access");

        // 5. Verify the extracted String
        assert_eq!(
            unsafe { string_ptr.as_const().get::<String>() },
            "try_into_inner test"
        );

        // 6. Clean up
        let string_drop_fn = (string_shape.vtable.sized().unwrap().drop_in_place)()
            .expect("String should have drop_in_place");

        unsafe {
            // The Arc should already be dropped by try_into_inner
            // But we still need to deallocate its memory
            arc_shape.deallocate_mut(arc_ptr)?;

            // Drop and deallocate the extracted String
            string_drop_fn(string_ptr);
            string_shape.deallocate_mut(string_ptr)?;
        }

        Ok(())
    }

    #[test]
    fn test_arc_vtable_6_slice_builder() -> eyre::Result<()> {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let arc_slice_shape = <Arc<[i32]>>::SHAPE;
        let arc_slice_def = arc_slice_shape
            .def
            .into_smart_pointer()
            .expect("Arc<[i32]> should have a smart pointer definition");

        // Get the slice builder vtable
        let slice_builder_vtable = arc_slice_def
            .vtable
            .slice_builder_vtable
            .expect("Arc<[i32]> should have slice_builder_vtable");

        // 1. Create a new builder
        let builder_ptr = (slice_builder_vtable.new_fn)();

        // 2. Push some items to the builder
        let push_fn = slice_builder_vtable.push_fn;
        let values = [1i32, 2, 3, 4, 5];
        for &value in &values {
            let mut value_copy = value;
            let value_ptr = PtrMut::new(&raw mut value_copy);
            unsafe { push_fn(builder_ptr, value_ptr) };
            let _ = value_copy; // Value now owned by the builder
        }

        // 3. Convert the builder to Arc<[i32]>
        let convert_fn = slice_builder_vtable.convert_fn;
        let arc_slice_ptr = unsafe { convert_fn(builder_ptr) };

        // 4. Verify the contents by borrowing
        let borrow_fn = arc_slice_def
            .vtable
            .borrow_fn
            .expect("Arc<[i32]> should have borrow_fn");
        let borrowed_ptr = unsafe { borrow_fn(arc_slice_ptr) };

        // Convert the wide pointer to a slice reference
        let slice = unsafe { borrowed_ptr.get::<[i32]>() };
        assert_eq!(slice, &[1, 2, 3, 4, 5]);

        // 5. Clean up - the Arc<[i32]> was boxed by convert_fn, we need to deallocate the Box
        unsafe {
            let _ = Box::from_raw(arc_slice_ptr.as_ptr::<Arc<[i32]>>() as *mut Arc<[i32]>);
        }

        Ok(())
    }

    #[test]
    fn test_arc_vtable_7_slice_builder_free() -> eyre::Result<()> {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let arc_slice_shape = <Arc<[String]>>::SHAPE;
        let arc_slice_def = arc_slice_shape
            .def
            .into_smart_pointer()
            .expect("Arc<[String]> should have a smart pointer definition");

        // Get the slice builder vtable
        let slice_builder_vtable = arc_slice_def
            .vtable
            .slice_builder_vtable
            .expect("Arc<[String]> should have slice_builder_vtable");

        // 1. Create a new builder
        let builder_ptr = (slice_builder_vtable.new_fn)();

        // 2. Push some items to the builder
        let push_fn = slice_builder_vtable.push_fn;
        let strings = ["hello", "world", "test"];
        for &s in &strings {
            let mut value = String::from(s);
            let value_ptr = PtrMut::new(&raw mut value);
            unsafe { push_fn(builder_ptr, value_ptr) };
            core::mem::forget(value); // Value now owned by the builder
        }

        // 3. Instead of converting, test the free function
        // This simulates abandoning the builder without creating the Arc
        let free_fn = slice_builder_vtable.free_fn;
        unsafe { free_fn(builder_ptr) };

        // If we get here without panicking, the free worked correctly
        Ok(())
    }
}
