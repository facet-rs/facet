use core::alloc::Layout;

use crate::{
    Def, Facet, KnownSmartPointer, PtrConst, PtrMut, PtrUninit, Shape, ShapeLayout,
    SmartPointerDef, SmartPointerFlags, SmartPointerVTable, TryBorrowInnerError, TryFromError,
    TryIntoInnerError, Type, UserType, ValueVTable, value_vtable,
};

unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for alloc::sync::Arc<T> {
    const VTABLE: &'static ValueVTable = &const {
        // Define the functions for transparent conversion between Arc<T> and T
        unsafe fn try_from<'a, 'src, 'dst, T: Facet<'a> + ?Sized>(
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

            let type_size = match T::SHAPE.layout {
                ShapeLayout::Sized(l) => l.size(),
                _ => panic!("can't try_from with unsized type"),
            };

            let mut arc: alloc::sync::Arc<[core::mem::MaybeUninit<u8>]> =
                alloc::sync::Arc::new_uninit_slice(type_size);
            {
                let arc_mut = alloc::sync::Arc::get_mut(&mut arc).unwrap();
                let arc_ptr = arc_mut.as_mut_ptr();
                let arc_ptr: *mut u8 = arc_ptr as *mut u8;
                unsafe {
                    core::ptr::copy_nonoverlapping(src_ptr.as_ptr::<u8>(), arc_ptr, type_size)
                };
            }

            Ok(unsafe { dst.put(arc) })
        }

        unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a> + ?Sized>(
            src_ptr: PtrConst<'src>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            // Copy the bytes of the inner value pointed by the Arc<T> into the destination.
            let mut arc = unsafe { src_ptr.read::<alloc::sync::Arc<T>>() };

            // For unsized types, we need to know how many bytes to copy.
            let type_size = match T::SHAPE.layout {
                ShapeLayout::Sized(layout) => layout.size(),
                _ => panic!("cannot try_into_inner with unsized type"),
            };

            // Get a pointer to the inner T held by the Arc<T> using Arc::get_mut.
            // Safety: We assume the caller has exclusive access to the Arc at this point.
            let arc_mut: Option<&mut T> = alloc::sync::Arc::get_mut(&mut arc);
            let src: *const u8 = match arc_mut {
                Some(t_mut) => t_mut as *mut T as *const u8,
                None => {
                    // Don't fallback to shared, just return an error.
                    return Err(TryIntoInnerError::Unavailable);
                }
            };
            let dst: *mut u8 = dst.as_mut_byte_ptr();

            // Actually copy the inner value bytes from Arc<T> to the destination memory.
            unsafe {
                core::ptr::copy_nonoverlapping(src, dst, type_size);
            }

            // Use PtrMut::new to construct a PtrMut from the destination pointer.
            Ok(PtrMut::new(dst))
        }

        unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a> + ?Sized>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let arc = unsafe { src_ptr.get::<alloc::sync::Arc<T>>() };
            Ok(PtrConst::new(&**arc))
        }

        let mut vtable = value_vtable!(alloc::sync::Arc<T>, |f, opts| {
            write!(f, "Arc")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (T::SHAPE.vtable.type_name)(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        });

        vtable.try_from = Some(try_from::<T>);
        vtable.try_into_inner = Some(try_into_inner::<T>);
        vtable.try_borrow_inner = Some(try_borrow_inner::<T>);
        vtable
    };

    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a> + ?Sized>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
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
                    .weak(|| <alloc::sync::Weak<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = Self::as_ptr(unsafe { this.get() });
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    use alloc::sync::Arc;

                                    let layout = match T::SHAPE.layout {
                                        ShapeLayout::Sized(layout) => layout,
                                        ShapeLayout::Unsized => panic!("nope"),
                                    };

                                    let size_of_arc_header = core::mem::size_of::<usize>() * 2;

                                    // we don't know the layout of dummy_arc, but we can tell its size and we can copy it
                                    // in front of the `PtrMut`
                                    let arc_layout = unsafe {
                                        Layout::from_size_align_unchecked(
                                            size_of_arc_header + layout.size(),
                                            layout.align(),
                                        )
                                    };
                                    let mem = unsafe { alloc::alloc::alloc(arc_layout) };

                                    unsafe {
                                        // Copy the Arc header (including refcounts, vtable pointers, etc.) from a freshly-allocated Arc<()>
                                        // so that the struct before the T value is a valid Arc header.
                                        let dummy_arc = alloc::sync::Arc::new(());
                                        let header_start = (Arc::as_ptr(&dummy_arc) as *const u8)
                                            .sub(size_of_arc_header);
                                        core::ptr::copy_nonoverlapping(
                                            header_start,
                                            mem,
                                            size_of_arc_header,
                                        );

                                        // Copy the value for T, pointed to by `ptr`, into the bytes just after the Arc header
                                        core::ptr::copy_nonoverlapping(
                                            ptr.as_byte_ptr(),
                                            mem.add(size_of_arc_header),
                                            layout.size(),
                                        );
                                    }

                                    // Safety: `mem` is valid and contains a valid Arc header and valid T.
                                    let ptr = unsafe { mem.add(size_of_arc_header) };
                                    let t_ptr: *mut T = unsafe { core::mem::transmute_copy(&ptr) };
                                    // Safety: This is the pointer to the Arc header + value; from_raw assumes a pointer to T located immediately after the Arc header.
                                    let arc = unsafe { Arc::from_raw(t_ptr) };
                                    // Move the Arc into the destination (this) and return a PtrMut for it.
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
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for alloc::sync::Weak<T> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::sync::Weak<T>, |f, opts| {
            write!(f, "Weak")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (T::SHAPE.vtable.type_name)(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        })
    };

    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a> + ?Sized>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
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
            .inner(inner_shape::<T>)
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
        let mut value = String::from("example");
        let arc1_ptr = unsafe { new_into_fn(arc1_uninit_ptr, PtrMut::new(&raw mut value)) };
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
}
