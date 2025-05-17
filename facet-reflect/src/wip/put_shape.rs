use facet_core::{Def, PtrConst, PtrMut, SequenceType, Shape, Type, UserType};
#[allow(unused_imports)]
use owo_colors::OwoColorize;

use crate::{ISet, ReflectError};
use crate::{debug, trace};

use super::Wip;

impl<'facet, 'shape> Wip<'facet, 'shape> {
    /// Puts a value from a `PtrConst` with the given shape into the current frame.
    pub fn put_shape(
        mut self,
        src: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<Wip<'facet, 'shape>, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: src_shape,
                operation: "tried to put a value but there was no frame to put into",
            });
        };

        // Check that the type matches
        if frame.shape != src_shape {
            trace!(
                "Trying to put a {} into a {}",
                src_shape.yellow(),
                frame.shape.magenta()
            );

            // Check if the frame's shape has an inner type (is a transparent wrapper)
            if let Some(inner_fn) = frame.shape.inner {
                // Get the inner shape
                let inner_shape = inner_fn();

                // If the source shape matches the inner shape, we need to build the outer (transparent) wrapper
                if src_shape == inner_shape {
                    // Look for a try_from_inner function in the vtable
                    if let Some(try_from_fn) = frame.shape.vtable.try_from {
                        match unsafe { (try_from_fn)(src, src_shape, frame.data) } {
                            Ok(_) => {
                                unsafe {
                                    frame.mark_fully_initialized();
                                }

                                let shape = frame.shape;
                                let index = frame.field_index_in_parent;

                                // mark the field as initialized
                                self.mark_field_as_initialized(shape, index)?;

                                debug!(
                                    "[{}] Just put a {} value into transparent type {}",
                                    self.frames.len(),
                                    src_shape.green(),
                                    shape.blue()
                                );

                                return Ok(self);
                            }
                            Err(e) => {
                                return Err(ReflectError::TryFromError {
                                    inner: e,
                                    src_shape,
                                    dst_shape: frame.shape,
                                });
                            }
                        }
                    } else {
                        // No try_from_inner function, try normal TryFrom
                        debug!(
                            "No try_from_inner function for transparent type, falling back to TryFrom"
                        );
                    }
                }
            }

            // Maybe there's a `TryFrom` impl?
            if let Some(try_from) = frame.shape.vtable.try_from {
                match unsafe { try_from(src, src_shape, frame.data) } {
                    Ok(_) => {
                        unsafe {
                            frame.mark_fully_initialized();
                        }

                        let shape = frame.shape;
                        let index = frame.field_index_in_parent;

                        // mark the field as initialized
                        self.mark_field_as_initialized(shape, index)?;

                        debug!("[{}] Just put a {} value", self.frames.len(), shape.green());

                        return Ok(self);
                    }
                    Err(e) => {
                        return Err(ReflectError::TryFromError {
                            inner: e,
                            src_shape,
                            dst_shape: frame.shape,
                        });
                    }
                }
            }

            // Maybe we're putting into an Option<T>?
            // Handle Option<Inner>
            if let Def::Option(od) = frame.shape.def {
                // Check if inner type matches
                if od.t() == src_shape {
                    debug!("Putting into an Option<T>!");
                    if frame.istate.fields.is_any_set() {
                        let data = unsafe { frame.data.assume_init() };
                        unsafe { (od.vtable.replace_with_fn)(data, Some(src)) };
                    } else {
                        let data = frame.data;
                        unsafe { (od.vtable.init_some_fn)(data, src) };
                    }
                    unsafe {
                        frame.mark_fully_initialized();
                    }

                    let shape = frame.shape;
                    let index = frame.field_index_in_parent;

                    // mark the field as initialized
                    self.mark_field_as_initialized(shape, index)?;

                    debug!("[{}] Just put a {} value", self.frames.len(), shape.green());

                    return Ok(self);
                }
            }

            // Maybe we're putting into a tuple struct, or a tuple, and it just so happens that the
            // first non-initialized field has the right type?
            {
                let fields = match &frame.shape.ty {
                    Type::User(UserType::Struct(sd)) => Some(sd.fields),
                    Type::Sequence(SequenceType::Tuple(tt)) => Some(tt.fields),
                    _ => None,
                };

                if let Some(fields) = fields {
                    // Look for the first uninitialized field
                    for (i, field) in fields.iter().enumerate() {
                        if !frame.istate.fields.has(i) {
                            // First, check for exact type match
                            if field.shape() == src_shape {
                                debug!(
                                    "Found uninitialized field {} with matching type {}",
                                    i.blue(),
                                    src_shape.green()
                                );

                                // Copy the value to the field
                                unsafe {
                                    let field_data = frame.data.field_uninit_at(field.offset);
                                    field_data.copy_from(src, field.shape()).map_err(|_| {
                                        ReflectError::Unsized {
                                            shape: field.shape(),
                                        }
                                    })?;
                                    frame.istate.fields.set(i);
                                }

                                let shape = frame.shape;
                                let index = frame.field_index_in_parent;

                                // If all fields are now initialized, mark the struct itself as initialized
                                if frame.is_fully_initialized() {
                                    self.mark_field_as_initialized(shape, index)?;
                                }

                                debug!(
                                    "[{}] Put a {} value into field {} of {}",
                                    self.frames.len(),
                                    src_shape.green(),
                                    i.blue(),
                                    shape.green()
                                );

                                return Ok(self);
                            }

                            // Then check if field's type has a try_from impl that can convert from src_shape
                            if let Some(try_from) = field.shape().vtable.try_from {
                                debug!(
                                    "Found uninitialized field {} with try_from for type {}",
                                    i.blue(),
                                    src_shape.green()
                                );

                                // Try to convert the value and store it in the field
                                let field_data =
                                    unsafe { frame.data.field_uninit_at(field.offset) };
                                match unsafe { try_from(src, src_shape, field_data) } {
                                    Ok(_) => {
                                        frame.istate.fields.set(i);

                                        let shape = frame.shape;
                                        let index = frame.field_index_in_parent;

                                        // If all fields are now initialized, mark the struct itself as initialized
                                        if frame.is_fully_initialized() {
                                            self.mark_field_as_initialized(shape, index)?;
                                        }

                                        debug!(
                                            "[{}] Put a {} value (converted) into field {} of {}",
                                            self.frames.len(),
                                            src_shape.green(),
                                            i.blue(),
                                            shape.green()
                                        );

                                        return Ok(self);
                                    }
                                    Err(_) => {
                                        // Conversion failed, try the next field
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Maybe we're putting into an enum, which has a variant selected, which has tuple-like fields,
            // and the first field that is uninitialized just so happens to be the right type?
            if let Type::User(UserType::Enum(_)) = frame.shape.ty {
                // Check if we're putting into an enum with a selected variant
                if let Some(variant) = &frame.istate.variant {
                    // Look for the first uninitialized field in the variant
                    for (i, field) in variant.data.fields.iter().enumerate() {
                        if !frame.istate.fields.has(i) {
                            // First, check for exact type match
                            if field.shape() == src_shape {
                                debug!(
                                    "Found uninitialized field {} in enum variant '{}' with matching type {}",
                                    i.blue(),
                                    variant.name.bright_yellow(),
                                    src_shape.green()
                                );

                                // Copy the value to the field
                                unsafe {
                                    let field_data = frame.data.field_uninit_at(field.offset);
                                    field_data.copy_from(src, field.shape()).map_err(|_| {
                                        ReflectError::Unsized {
                                            shape: field.shape(),
                                        }
                                    })?;
                                    frame.istate.fields.set(i);
                                }

                                let shape = frame.shape;
                                let index = frame.field_index_in_parent;

                                #[allow(unused)]
                                let variant_name = variant.name;

                                // If all fields are now initialized, mark the enum itself as initialized
                                if frame.is_fully_initialized() {
                                    self.mark_field_as_initialized(shape, index)?;
                                }

                                debug!(
                                    "[{}] Put a {} value into field {} of variant '{}' in enum {}",
                                    self.frames.len(),
                                    src_shape.green(),
                                    i.blue(),
                                    variant_name.bright_yellow(),
                                    shape.green()
                                );

                                return Ok(self);
                            }

                            // Then check if field's type has a try_from impl that can convert from src_shape
                            if let Some(try_from) = field.shape().vtable.try_from {
                                debug!(
                                    "Found uninitialized field {} in enum variant '{}' with try_from for type {}",
                                    i.blue(),
                                    variant.name.bright_yellow(),
                                    src_shape.green()
                                );

                                // Try to convert the value and store it in the field
                                let field_data =
                                    unsafe { frame.data.field_uninit_at(field.offset) };
                                match unsafe { try_from(src, src_shape, field_data) } {
                                    Ok(_) => {
                                        frame.istate.fields.set(i);

                                        let shape = frame.shape;
                                        let index = frame.field_index_in_parent;

                                        #[allow(unused_variables)]
                                        let variant_name = variant.name;

                                        // If all fields are now initialized, mark the enum itself as initialized
                                        if frame.is_fully_initialized() {
                                            self.mark_field_as_initialized(shape, index)?;
                                        }

                                        debug!(
                                            "[{}] Put a {} value (converted) into field {} of variant '{}' in enum {}",
                                            self.frames.len(),
                                            src_shape.green(),
                                            i.blue(),
                                            variant_name.bright_yellow(),
                                            shape.green()
                                        );

                                        return Ok(self);
                                    }
                                    Err(_) => {
                                        // Conversion failed, try the next field
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            return Err(ReflectError::WrongShape {
                expected: frame.shape,
                actual: src_shape,
            });
        }

        // de-initialize partially initialized fields, if any
        if frame.istate.variant.is_some() || frame.istate.fields.is_any_set() {
            debug!("De-initializing partially initialized {:?}", frame.yellow());

            match frame.shape.ty {
                Type::User(UserType::Struct(sd)) => {
                    for (i, field) in sd.fields.iter().enumerate() {
                        if frame.istate.fields.has(i) {
                            if let Some(drop_fn) = field.shape().vtable.drop_in_place {
                                unsafe {
                                    let field_ptr = frame.data.as_mut_byte_ptr().add(field.offset);
                                    drop_fn(PtrMut::new(field_ptr));
                                }
                            }
                        }
                    }
                }
                Type::User(UserType::Enum(_)) => {
                    if let Some(variant) = &frame.istate.variant {
                        for (i, field) in variant.data.fields.iter().enumerate() {
                            if frame.istate.fields.has(i) {
                                if let Some(drop_fn) = field.shape().vtable.drop_in_place {
                                    unsafe {
                                        let field_ptr =
                                            frame.data.as_mut_byte_ptr().add(field.offset);
                                        drop_fn(PtrMut::new(field_ptr));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    // For scalar types and other non-struct/enum, attempt to drop the field in place if initialized
                    if frame.istate.fields.is_any_set() {
                        debug!("Scalar type was set...");
                        if let Some(drop_fn) = frame.shape.vtable.drop_in_place {
                            debug!("And it has a drop fn, dropping now!");
                            unsafe {
                                drop_fn(frame.data.assume_init());
                            }
                        }
                    }
                }
            }

            // Reset initialization state
            frame.istate.variant = None;
            ISet::clear(&mut frame.istate.fields);
        }

        unsafe {
            // Copy the contents from src to destination
            frame
                .data
                .copy_from(src, frame.shape)
                .map_err(|_| ReflectError::Unsized { shape: frame.shape })?;
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        debug!("[{}] Just put a {} value", self.frames.len(), shape.green());

        Ok(self)
    }
}
