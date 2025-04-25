use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, KnownSmartPointer, Opaque, PtrConst, PtrMut, PtrUninit, Shape,
    SmartPointerDef, SmartPointerFlags, SmartPointerVTable, TryBorrowInnerError, TryFromError,
    TryIntoInnerError, value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for alloc::sync::Arc<T> {
    const SHAPE: &'static crate::Shape = &const {
        // Define the functions for transparent conversion between Arc<T> and T
        unsafe fn try_from<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError> {
            if src_shape.id != T::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[T::SHAPE],
                });
            }
            let t = unsafe { src_ptr.read::<T>() };
            let arc = alloc::sync::Arc::new(t);
            Ok(unsafe { dst.put(arc) })
        }

        unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let arc = unsafe { src_ptr.get::<alloc::sync::Arc<T>>() };
            match alloc::sync::Arc::try_unwrap(arc.clone()) {
                Ok(t) => Ok(unsafe { dst.put(t) }),
                Err(_) => Err(TryIntoInnerError::Unavailable),
            }
        }

        unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let arc = unsafe { src_ptr.get::<alloc::sync::Arc<T>>() };
            Ok(PtrConst::new(&**arc))
        }

        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(T::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC)
                    .known(KnownSmartPointer::Arc)
                    .weak(|| <alloc::sync::Weak<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = Self::as_ptr(unsafe { this.get() });
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let arc = alloc::sync::Arc::new(t);
                                    unsafe { this.put(arc) }
                                })
                                .downgrade_into_fn(|strong, weak| unsafe {
                                    weak.put(alloc::sync::Arc::downgrade(strong.get::<Self>()))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable =
                        value_vtable!(alloc::sync::Arc<T>, |f, _opts| write!(f, "Arc"));
                    vtable.try_from = Some(try_from::<T>);
                    vtable.try_into_inner = Some(try_into_inner::<T>);
                    vtable.try_borrow_inner = Some(try_borrow_inner::<T>);
                    vtable.drop_in_place =
                        Some(|ptr| unsafe { ptr.drop_in_place::<alloc::sync::Arc<T>>() });
                    vtable
                },
            )
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for alloc::sync::Weak<T> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(T::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC.union(SmartPointerFlags::WEAK))
                    .known(KnownSmartPointer::ArcWeak)
                    .strong(|| <alloc::sync::Arc<T> as Facet>::SHAPE)
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
            .vtable(
                &const {
                    let mut vtable =
                        value_vtable!(alloc::sync::Arc<T>, |f, _opts| write!(f, "Arc"));
                    vtable.drop_in_place =
                        Some(|ptr| unsafe { ptr.drop_in_place::<alloc::sync::Weak<T>>() });

                    vtable
                },
            )
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a, T: 'a> Facet<'a> for Opaque<alloc::sync::Arc<T>> {
    const SHAPE: &'static crate::Shape = &const {
        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .flags(SmartPointerFlags::ATOMIC)
                    .known(KnownSmartPointer::Arc)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = alloc::sync::Arc::<T>::as_ptr(unsafe { this.get() });
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let arc = alloc::sync::Arc::new(t);
                                    unsafe { this.put(arc) }
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .vtable(&const { value_vtable!(alloc::sync::Arc<T>, |f, _opts| write!(f, "Arc")) })
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for alloc::rc::Rc<T> {
    const SHAPE: &'static crate::Shape = &const {
        // Define the functions for transparent conversion between Rc<T> and T
        unsafe fn try_from<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError> {
            if src_shape.id != T::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[T::SHAPE],
                });
            }
            let t = unsafe { src_ptr.read::<T>() };
            let rc = alloc::rc::Rc::new(t);
            Ok(unsafe { dst.put(rc) })
        }

        unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let rc = unsafe { src_ptr.get::<alloc::rc::Rc<T>>() };
            match alloc::rc::Rc::try_unwrap(rc.clone()) {
                Ok(t) => Ok(unsafe { dst.put(t) }),
                Err(_) => Err(TryIntoInnerError::Unavailable),
            }
        }

        unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let rc = unsafe { src_ptr.get::<alloc::rc::Rc<T>>() };
            Ok(PtrConst::new(&**rc))
        }

        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(T::SHAPE)
                    .flags(SmartPointerFlags::EMPTY)
                    .known(KnownSmartPointer::Rc)
                    .weak(|| <alloc::rc::Weak<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = Self::as_ptr(unsafe { this.get() });
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let rc = alloc::rc::Rc::new(t);
                                    unsafe { this.put(rc) }
                                })
                                .downgrade_into_fn(|strong, weak| unsafe {
                                    weak.put(alloc::rc::Rc::downgrade(strong.get::<Self>()))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable = value_vtable!(alloc::rc::Rc<T>, |f, _opts| write!(f, "Rc"));
                    vtable.try_from = Some(try_from::<T>);
                    vtable.try_into_inner = Some(try_into_inner::<T>);
                    vtable.try_borrow_inner = Some(try_borrow_inner::<T>);
                    vtable.drop_in_place =
                        Some(|ptr| unsafe { ptr.drop_in_place::<alloc::rc::Rc<T>>() });
                    vtable
                },
            )
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for alloc::rc::Weak<T> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(T::SHAPE)
                    .flags(SmartPointerFlags::WEAK)
                    .known(KnownSmartPointer::RcWeak)
                    .strong(|| <alloc::rc::Rc<T> as Facet>::SHAPE)
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
            .vtable(
                &const {
                    let mut vtable =
                        value_vtable!(alloc::rc::Weak<T>, |f, _opts| write!(f, "Weak")); // Note: Weak<T> for type name
                    vtable.drop_in_place =
                        Some(|ptr| unsafe { ptr.drop_in_place::<alloc::rc::Weak<T>>() });
                    vtable
                },
            )
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a, T: 'a> Facet<'a> for Opaque<alloc::rc::Rc<T>> {
    const SHAPE: &'static crate::Shape = &const {
        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .known(KnownSmartPointer::Rc)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = alloc::rc::Rc::<T>::as_ptr(unsafe { this.get() });
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let rc = alloc::rc::Rc::new(t);
                                    unsafe { this.put(rc) }
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .vtable(&const { value_vtable!(alloc::rc::Rc<T>, |f, _opts| write!(f, "Rc")) })
            .build()
    };
}

#[cfg(test)]
mod tests {
    use alloc::rc::{Rc, Weak as RcWeak};
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
        let value = String::from("example");
        let arc_ptr = unsafe { new_into_fn(arc_uninit_ptr, PtrConst::new(&raw const value)) };
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
        let drop_fn = arc_shape
            .vtable
            .drop_in_place
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
        let value = String::from("example");
        let arc1_ptr = unsafe { new_into_fn(arc1_uninit_ptr, PtrConst::new(&raw const value)) };
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
        let arc_drop_fn = arc_shape.vtable.drop_in_place.unwrap();
        let weak_drop_fn = weak_shape.vtable.drop_in_place.unwrap();

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
        let value = String::from("example");
        let arc1_ptr = unsafe { new_into_fn(arc1_uninit_ptr, PtrConst::new(&raw const value)) };
        core::mem::forget(value);

        // 2. Downgrade arc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate()?;
        let downgrade_into_fn = arc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: arc1_ptr is valid, weak1_uninit_ptr is allocated for Weak
        let weak1_ptr = unsafe { downgrade_into_fn(arc1_ptr, weak1_uninit_ptr) };

        // 3. Drop and free the strong pointer (arc1)
        let arc_drop_fn = arc_shape.vtable.drop_in_place.unwrap();
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
        let weak_drop_fn = weak_shape.vtable.drop_in_place.unwrap();
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
    fn test_rc_type_params() {
        let [type_param_1] = <Rc<i32>>::SHAPE.type_params else {
            panic!("Rc<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_rc_vtable_1_new_borrow_drop() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let rc_shape = <Rc<String>>::SHAPE;
        let rc_def = rc_shape
            .def
            .into_smart_pointer()
            .expect("Rc<T> should have a smart pointer definition");

        // Allocate memory for the Rc
        let rc_uninit_ptr = rc_shape.allocate()?;

        // Get the function pointer for creating a new Rc from a value
        let new_into_fn = rc_def
            .vtable
            .new_into_fn
            .expect("Rc<T> should have new_into_fn");

        // Create the value and initialize the Rc
        let value = String::from("example");
        let rc_ptr = unsafe { new_into_fn(rc_uninit_ptr, PtrConst::new(&raw const value)) };
        // The value now belongs to the Rc, prevent its drop
        core::mem::forget(value);

        // Get the function pointer for borrowing the inner value
        let borrow_fn = rc_def
            .vtable
            .borrow_fn
            .expect("Rc<T> should have borrow_fn");

        // Borrow the inner value and check it
        let borrowed_ptr = unsafe { borrow_fn(rc_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String within the Rc
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // Get the function pointer for dropping the Rc
        let drop_fn = rc_shape
            .vtable
            .drop_in_place
            .expect("Rc<T> should have drop_in_place");

        // Drop the Rc in place
        // SAFETY: rc_ptr points to a valid Rc<String>
        unsafe { drop_fn(rc_ptr) };

        // Deallocate the memory
        // SAFETY: rc_ptr was allocated by rc_shape and is now dropped (but memory is still valid)
        unsafe { rc_shape.deallocate_mut(rc_ptr)? };

        Ok(())
    }

    #[test]
    fn test_rc_vtable_2_downgrade_upgrade_drop() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let rc_shape = <Rc<String>>::SHAPE;
        let rc_def = rc_shape
            .def
            .into_smart_pointer()
            .expect("Rc<T> should have a smart pointer definition");

        let weak_shape = <RcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_smart_pointer()
            .expect("RcWeak<T> should have a smart pointer definition");

        // 1. Create the first Rc (rc1)
        let rc1_uninit_ptr = rc_shape.allocate()?;
        let new_into_fn = rc_def.vtable.new_into_fn.unwrap();
        let value = String::from("example");
        let rc1_ptr = unsafe { new_into_fn(rc1_uninit_ptr, PtrConst::new(&raw const value)) };
        core::mem::forget(value); // Value now owned by rc1

        // 2. Downgrade rc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate()?;
        let downgrade_into_fn = rc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: rc1_ptr points to a valid Rc, weak1_uninit_ptr is allocated for a Weak
        let weak1_ptr = unsafe { downgrade_into_fn(rc1_ptr, weak1_uninit_ptr) };

        // 3. Upgrade weak1 to create a second Rc (rc2)
        let rc2_uninit_ptr = rc_shape.allocate()?;
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        // SAFETY: weak1_ptr points to a valid Weak, rc2_uninit_ptr is allocated for an Rc.
        // Upgrade should succeed as rc1 still exists.
        let rc2_ptr = unsafe { upgrade_into_fn(weak1_ptr, rc2_uninit_ptr) }
            .expect("Upgrade should succeed while original Rc exists");

        // Check the content of the upgraded Rc
        let borrow_fn = rc_def.vtable.borrow_fn.unwrap();
        // SAFETY: rc2_ptr points to a valid Rc<String>
        let borrowed_ptr = unsafe { borrow_fn(rc2_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // 4. Drop everything and free memory
        let rc_drop_fn = rc_shape.vtable.drop_in_place.unwrap();
        let weak_drop_fn = weak_shape.vtable.drop_in_place.unwrap();

        unsafe {
            // Drop Rcs
            rc_drop_fn(rc1_ptr);
            rc_shape.deallocate_mut(rc1_ptr)?;
            rc_drop_fn(rc2_ptr);
            rc_shape.deallocate_mut(rc2_ptr)?;

            // Drop Weak
            weak_drop_fn(weak1_ptr);
            weak_shape.deallocate_mut(weak1_ptr)?;
        }

        Ok(())
    }

    #[test]
    fn test_rc_vtable_3_downgrade_drop_try_upgrade() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let rc_shape = <Rc<String>>::SHAPE;
        let rc_def = rc_shape
            .def
            .into_smart_pointer()
            .expect("Rc<T> should have a smart pointer definition");

        let weak_shape = <RcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_smart_pointer()
            .expect("RcWeak<T> should have a smart pointer definition");

        // 1. Create the strong Rc (rc1)
        let rc1_uninit_ptr = rc_shape.allocate()?;
        let new_into_fn = rc_def.vtable.new_into_fn.unwrap();
        let value = String::from("example");
        let rc1_ptr = unsafe { new_into_fn(rc1_uninit_ptr, PtrConst::new(&raw const value)) };
        core::mem::forget(value);

        // 2. Downgrade rc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate()?;
        let downgrade_into_fn = rc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: rc1_ptr is valid, weak1_uninit_ptr is allocated for Weak
        let weak1_ptr = unsafe { downgrade_into_fn(rc1_ptr, weak1_uninit_ptr) };

        // 3. Drop and free the strong pointer (rc1)
        let rc_drop_fn = rc_shape.vtable.drop_in_place.unwrap();
        unsafe {
            rc_drop_fn(rc1_ptr);
            rc_shape.deallocate_mut(rc1_ptr)?;
        }

        // 4. Attempt to upgrade the weak pointer (weak1)
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        let rc2_uninit_ptr = rc_shape.allocate()?;
        // SAFETY: weak1_ptr is valid (though points to dropped data), rc2_uninit_ptr is allocated for Rc
        let upgrade_result = unsafe { upgrade_into_fn(weak1_ptr, rc2_uninit_ptr) };

        // Assert that the upgrade failed
        assert!(
            upgrade_result.is_none(),
            "Upgrade should fail after the strong Rc is dropped"
        );

        // 5. Clean up: Deallocate the memory intended for the failed upgrade and drop/deallocate the weak pointer
        let weak_drop_fn = weak_shape.vtable.drop_in_place.unwrap();
        unsafe {
            // Deallocate the *uninitialized* memory allocated for the failed upgrade attempt
            rc_shape.deallocate_uninit(rc2_uninit_ptr)?;

            // Drop and deallocate the weak pointer
            weak_drop_fn(weak1_ptr);
            weak_shape.deallocate_mut(weak1_ptr)?;
        }

        Ok(())
    }
}
