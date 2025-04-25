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
            .vtable(&const { value_vtable!(alloc::sync::Arc<T>, |f, _opts| write!(f, "Arc")) })
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
            .vtable(&const { value_vtable!(alloc::rc::Rc<T>, |f, _opts| write!(f, "Rc")) })
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
    use core::mem::MaybeUninit;

    use super::*;

    use crate::PtrUninit;

    #[test]
    fn test_arc_type_params() {
        let [type_param_1] = <Arc<i32>>::SHAPE.type_params else {
            panic!("Arc<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_arc_vtable() {
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

        // Keep this alive as long as the Arc inside it is used
        let mut arc_storage = MaybeUninit::<Arc<String>>::zeroed();
        let arc_ptr = unsafe {
            let arc_uninit_ptr = PtrUninit::from_maybe_uninit(&mut arc_storage);

            let value = String::from("example");
            let value_ptr = PtrConst::new(&raw const value);

            // SAFETY:
            // - `arc_uninit_ptr` has layout Arc<String>
            // - `value_ptr` is String
            // - `value_ptr` is deallocated
            let returned_ptr = arc_def
                .vtable
                .new_into_fn
                .expect("Arc<T> should have new_into_fn vtable function")(
                arc_uninit_ptr,
                value_ptr,
            );

            // Don't run the destructor
            core::mem::forget(value);

            // Test correctness of the return value of new_into_fn
            // SAFETY: Using correct type Arc<String>
            assert_eq!(
                returned_ptr.as_ptr(),
                arc_uninit_ptr.as_byte_ptr() as *const Arc<String>
            );

            returned_ptr
        };

        unsafe {
            // SAFETY: `arc_ptr` is valid
            let borrowed = arc_def
                .vtable
                .borrow_fn
                .expect("Arc<T> should have borrow_fn vtable function")(
                arc_ptr.as_const()
            );
            assert_eq!(borrowed.get::<String>(), "example");
        }

        // Keep this alive as long as the RcWeak inside it is used
        let mut new_arc_storage = MaybeUninit::<ArcWeak<String>>::zeroed();
        let weak_ptr = unsafe {
            let weak_uninit_ptr = PtrUninit::from_maybe_uninit(&mut new_arc_storage);

            let returned_ptr = arc_def
                .vtable
                .downgrade_into_fn
                .expect("Arc<T> should have downgrade_into_fn vtable function")(
                arc_ptr,
                weak_uninit_ptr,
            );

            // Test correctness of the return value of downgrade_into_fn
            // SAFETY: Using correct type ArcWeak<String>
            assert_eq!(
                returned_ptr.as_ptr(),
                weak_uninit_ptr.as_byte_ptr() as *const ArcWeak<String>
            );

            returned_ptr
        };

        {
            let mut new_arc_storage = MaybeUninit::<Arc<String>>::zeroed();
            let new_arc_ptr = unsafe {
                let new_arc_uninit_ptr = PtrUninit::from_maybe_uninit(&mut new_arc_storage);

                // SAFETY: `weak_ptr` is valid and `new_arc_uninit_ptr` has layout Weak<String>
                let returned_ptr = weak_def
                    .vtable
                    .upgrade_into_fn
                    .expect("ArcWeak<T> should have upgrade_into_fn vtable function")(
                    weak_ptr,
                    new_arc_uninit_ptr,
                )
                .expect("Upgrade should be successful");

                // Test correctness of the return value of upgrade_into_fn
                // SAFETY: Using correct type Arc<String>
                assert_eq!(
                    returned_ptr.as_ptr(),
                    new_arc_uninit_ptr.as_byte_ptr() as *const Arc<String>
                );

                returned_ptr
            };

            unsafe {
                // SAFETY: `new_arc_ptr` is valid
                let borrowed = arc_def
                    .vtable
                    .borrow_fn
                    .expect("Arc<T> should have borrow_fn vtable function")(
                    new_arc_ptr.as_const()
                );
                assert_eq!(borrowed.get::<String>(), "example");
            }

            unsafe {
                // SAFETY: Proper value at `arc_ptr`, which is not accessed after this
                arc_shape
                    .vtable
                    .drop_in_place
                    .expect("Arc<T> should have drop_in_place vtable function")(
                    new_arc_ptr
                );
            }
        }

        unsafe {
            // SAFETY: Proper value at `arc_ptr`, which is not accessed after this
            arc_shape
                .vtable
                .drop_in_place
                .expect("Arc<T> should have drop_in_place vtable function")(arc_ptr);
        }

        unsafe {
            let mut new_arc_storage = MaybeUninit::<Arc<String>>::zeroed();
            let new_arc_uninit_ptr = PtrUninit::from_maybe_uninit(&mut new_arc_storage);

            // SAFETY: `weak_ptr` is valid and `new_arc_uninit_ptr` has layout Weak<String>
            if weak_def
                .vtable
                .upgrade_into_fn
                .expect("ArcWeak<T> should have upgrade_into_fn vtable function")(
                weak_ptr,
                new_arc_uninit_ptr,
            )
            .is_some()
            {
                panic!("Upgrade should be unsuccessful")
            }
        };

        unsafe {
            // SAFETY: Proper value at `weak_ptr`, which is not accessed after this
            weak_shape
                .vtable
                .drop_in_place
                .expect("ArcWeak<T> should have drop_in_place vtable function")(
                weak_ptr
            );
        }
    }

    #[test]
    fn test_rc_type_params() {
        let [type_param_1] = <Rc<i32>>::SHAPE.type_params else {
            panic!("Rc<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_rc_vtable() {
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

        // Keep this alive as long as the Rc inside it is used
        let mut rc_storage = MaybeUninit::<Rc<String>>::zeroed();
        let rc_ptr = unsafe {
            let rc_uninit_ptr = PtrUninit::from_maybe_uninit(&mut rc_storage);

            let value = String::from("example");
            let value_ptr = PtrConst::new(&raw const value);

            // SAFETY:
            // - `rc_uninit_ptr` has layout Rc<String>
            // - `value_ptr` is String
            // - `value_ptr` is deallocated after this without running the destructor
            let returned_ptr = rc_def
                .vtable
                .new_into_fn
                .expect("Rc<T> should have new_into_fn vtable function")(
                rc_uninit_ptr, value_ptr
            );

            // Don't run the destructor
            core::mem::forget(value);

            // Test correctness of the return value of new_into_fn
            // SAFETY: Using correct type Rc<String>
            assert_eq!(
                returned_ptr.as_ptr(),
                rc_uninit_ptr.as_byte_ptr() as *const Rc<String>
            );

            returned_ptr
        };

        unsafe {
            // SAFETY: `rc_ptr` is valid
            let borrowed = rc_def
                .vtable
                .borrow_fn
                .expect("Rc<T> should have borrow_fn vtable function")(
                rc_ptr.as_const()
            );
            assert_eq!(borrowed.get::<String>(), "example");
        }

        // Keep this alive as long as the RcWeak inside it is used
        let mut new_rc_storage = MaybeUninit::<RcWeak<String>>::zeroed();
        let weak_ptr = unsafe {
            let weak_uninit_ptr = PtrUninit::from_maybe_uninit(&mut new_rc_storage);

            let returned_ptr = rc_def
                .vtable
                .downgrade_into_fn
                .expect("Rc<T> should have downgrade_into_fn vtable function")(
                rc_ptr,
                weak_uninit_ptr,
            );

            // Test correctness of the return value of downgrade_into_fn
            // SAFETY: Using correct type RcWeak<String>
            assert_eq!(
                returned_ptr.as_ptr(),
                weak_uninit_ptr.as_byte_ptr() as *const RcWeak<String>
            );

            returned_ptr
        };

        {
            let mut new_rc_storage = MaybeUninit::<Rc<String>>::zeroed();
            let new_rc_ptr = unsafe {
                let new_rc_uninit_ptr = PtrUninit::from_maybe_uninit(&mut new_rc_storage);

                // SAFETY: `weak_ptr` is valid and `new_rc_uninit_ptr` has layout Weak<String>
                let returned_ptr = weak_def
                    .vtable
                    .upgrade_into_fn
                    .expect("RcWeak<T> should have upgrade_into_fn vtable function")(
                    weak_ptr,
                    new_rc_uninit_ptr,
                )
                .expect("Upgrade should be successful");

                // Test correctness of the return value of upgrade_into_fn
                // SAFETY: Using correct type Rc<String>
                assert_eq!(
                    returned_ptr.as_ptr(),
                    new_rc_uninit_ptr.as_byte_ptr() as *const Rc<String>
                );

                returned_ptr
            };

            unsafe {
                // SAFETY: `new_rc_ptr` is valid
                let borrowed = rc_def
                    .vtable
                    .borrow_fn
                    .expect("Rc<T> should have borrow_fn vtable function")(
                    new_rc_ptr.as_const()
                );
                assert_eq!(borrowed.get::<String>(), "example");
            }

            unsafe {
                // SAFETY: Proper value at `rc_ptr`, which is not accessed after this
                rc_shape
                    .vtable
                    .drop_in_place
                    .expect("Rc<T> should have drop_in_place vtable function")(
                    new_rc_ptr
                );
            }
        }

        unsafe {
            // SAFETY: Proper value at `rc_ptr`, which is not accessed after this
            rc_shape
                .vtable
                .drop_in_place
                .expect("Rc<T> should have drop_in_place vtable function")(rc_ptr);
        }

        unsafe {
            let mut new_rc_storage = MaybeUninit::<Rc<String>>::zeroed();
            let new_rc_uninit_ptr = PtrUninit::from_maybe_uninit(&mut new_rc_storage);

            // SAFETY: `weak_ptr` is valid and `new_rc_uninit_ptr` has layout Weak<String>
            if weak_def
                .vtable
                .upgrade_into_fn
                .expect("RcWeak<T> should have upgrade_into_fn vtable function")(
                weak_ptr,
                new_rc_uninit_ptr,
            )
            .is_some()
            {
                panic!("Upgrade should be unsuccessful")
            }
        };

        unsafe {
            // SAFETY: Proper value at `weak_ptr`, which is not accessed after this
            weak_shape
                .vtable
                .drop_in_place
                .expect("RcWeak<T> should have drop_in_place vtable function")(weak_ptr);
        }
    }
}
