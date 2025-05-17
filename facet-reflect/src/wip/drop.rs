use alloc::{vec, vec::Vec};
use facet_core::{Type, UserType};

#[allow(unused_imports)]
use owo_colors::OwoColorize;

use super::Wip;
use crate::{FrameFlags, FrameMode, Guard, ValueId, trace, wip::frame::Frame};

impl<'facet_lifetime, 'shape> Drop for Wip<'facet_lifetime, 'shape> {
    fn drop(&mut self) {
        trace!("🧹🧹🧹 WIP is dropping");

        while let Some(frame) = self.frames.pop() {
            self.track(frame);
        }

        let Some((root_id, _)) = self.istates.iter().find(|(_k, istate)| istate.depth == 0) else {
            trace!("No root found, we probably built already");
            return;
        };

        let root_id = *root_id;
        let root_istate = self.istates.remove(&root_id).unwrap();
        let root = Frame::recompose(root_id, root_istate);
        let mut to_clean = vec![root];

        let mut _root_guard: Option<Guard> = None;

        while let Some(mut frame) = to_clean.pop() {
            trace!(
                "Cleaning frame: shape={} at {:p}, flags={:?}, mode={:?}, fully_initialized={}",
                frame.shape.blue(),
                frame.data.as_byte_ptr(),
                frame.istate.flags.bright_magenta(),
                frame.istate.mode.yellow(),
                if frame.is_fully_initialized() {
                    "✅"
                } else {
                    "❌"
                }
            );

            if frame.istate.flags.contains(FrameFlags::MOVED) {
                trace!(
                    "{}",
                    "Frame was moved out of, nothing to dealloc/drop_in_place".yellow()
                );
                continue;
            }

            match frame.shape.ty {
                Type::User(UserType::Struct(sd)) => {
                    if frame.is_fully_initialized() {
                        trace!(
                            "Dropping fully initialized struct: {} at {:p}",
                            frame.shape.green(),
                            frame.data.as_byte_ptr()
                        );
                        let frame = self.evict_tree(frame);
                        unsafe { frame.drop_and_dealloc_if_needed() };
                    } else {
                        let num_fields = sd.fields.len();
                        trace!(
                            "De-initializing struct {} at {:p} field-by-field ({} fields)",
                            frame.shape.yellow(),
                            frame.data.as_byte_ptr(),
                            num_fields.bright_cyan()
                        );
                        for i in 0..num_fields {
                            if frame.istate.fields.has(i) {
                                let field = sd.fields[i];
                                let field_shape = field.shape();
                                let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                                let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());
                                trace!(
                                    "Recursively cleaning field #{} '{}' of {}: field_shape={}, field_ptr={:p}",
                                    i.bright_cyan(),
                                    field.name.bright_blue(),
                                    frame.shape.blue(),
                                    field_shape.green(),
                                    field_ptr.as_byte_ptr()
                                );
                                let istate = self.istates.remove(&field_id).unwrap();
                                let field_frame = Frame::recompose(field_id, istate);
                                to_clean.push(field_frame);
                            } else {
                                trace!(
                                    "Field #{} '{}' of {} was NOT initialized, skipping",
                                    i.bright_cyan(),
                                    sd.fields[i].name.bright_red(),
                                    frame.shape.red()
                                );
                            }
                        }

                        // we'll also need to clean up if we're root
                        if frame.istate.mode == FrameMode::Root {
                            if let Ok(layout) = frame.shape.layout.sized_layout() {
                                _root_guard = Some(Guard {
                                    ptr: frame.data.as_mut_byte_ptr(),
                                    layout,
                                });
                            }
                        }
                    }
                }
                Type::User(UserType::Enum(_ed)) => {
                    trace!(
                        "Handling enum deallocation for {} at {:p}",
                        frame.shape.yellow(),
                        frame.data.as_byte_ptr()
                    );

                    // Check if a variant is selected
                    if let Some(variant) = &frame.istate.variant {
                        trace!(
                            "Dropping enum variant {} of {} with {} fields",
                            variant.name.bright_yellow(),
                            frame.shape.yellow(),
                            variant.data.fields.len()
                        );

                        // Recursively clean fields of the variant that are initialized
                        for (i, field) in variant.data.fields.iter().enumerate() {
                            if frame.istate.fields.has(i) {
                                let field_shape = field.shape();
                                let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                                let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());
                                trace!(
                                    "Recursively cleaning field #{} '{}' of variant {}: field_shape={}, field_ptr={:p}",
                                    i.bright_cyan(),
                                    field.name.bright_blue(),
                                    variant.name.yellow(),
                                    field_shape.green(),
                                    field_ptr.as_byte_ptr()
                                );
                                if let Some(istate) = self.istates.remove(&field_id) {
                                    let field_frame = Frame::recompose(field_id, istate);
                                    to_clean.push(field_frame);
                                } else {
                                    trace!(
                                        "Field not found in istates: #{} '{}' of variant {}",
                                        i.bright_cyan(),
                                        field.name.bright_blue(),
                                        variant.name.yellow()
                                    );
                                    // that means it's fully initialized and we need to drop it
                                    unsafe {
                                        if let Some(drop_in_place) =
                                            field_shape.vtable.drop_in_place
                                        {
                                            drop_in_place(field_ptr);
                                        }
                                    }
                                }
                            } else {
                                trace!(
                                    "Field #{} '{}' of variant {} was NOT initialized, skipping",
                                    i.bright_cyan(),
                                    field.name.bright_red(),
                                    variant.name.yellow()
                                );
                            }
                        }
                    } else {
                        trace!(
                            "Enum {} has no variant selected, nothing to drop for fields",
                            frame.shape.yellow()
                        );
                    }

                    // we'll also need to clean up if we're root
                    if frame.istate.mode == FrameMode::Root {
                        if let Ok(layout) = frame.shape.layout.sized_layout() {
                            _root_guard = Some(Guard {
                                ptr: frame.data.as_mut_byte_ptr(),
                                layout,
                            });
                        }
                    }
                }
                _ => {
                    trace!(
                        "Can drop all at once for shape {} (frame mode {:?}) at {:p}",
                        frame.shape.cyan(),
                        frame.istate.mode.yellow(),
                        frame.data.as_byte_ptr(),
                    );

                    if frame.is_fully_initialized() {
                        unsafe { frame.drop_and_dealloc_if_needed() }
                    } else {
                        frame.dealloc_if_needed();
                    }
                }
            }
        }

        // We might have some frames left over to deallocate for temporary allocations for keymap insertion etc.
        let mut all_ids = self.istates.keys().copied().collect::<Vec<_>>();
        for frame_id in all_ids.drain(..) {
            let frame_istate = self.istates.remove(&frame_id).unwrap();

            trace!(
                "Checking leftover istate: id.shape={} id.ptr={:p} mode={:?}",
                frame_id.shape.cyan(),
                frame_id.ptr.red(),
                frame_istate.mode.yellow()
            );
            let mut frame = Frame::recompose(frame_id, frame_istate);

            if frame.is_fully_initialized() {
                trace!("It's fully initialized, we can drop it");
                unsafe { frame.drop_and_dealloc_if_needed() };
            } else if frame.istate.flags.contains(FrameFlags::ALLOCATED) {
                trace!("Not initialized but allocated, let's free it");
                frame.dealloc_if_needed();
            }
        }
    }
}
