use core::fmt;

use crate::{Def, Shape, ShapeLayout, TypeParam};

// Helper struct to format the name for display
impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: All Shape instances are guaranteed to be 'static because they're
        // always references to const statics created by the Facet derive macro or
        // manual implementations. The lifetime is erased in the Display trait signature,
        // but we can safely restore it here.
        let static_self: &'static Shape = unsafe { core::mem::transmute(self) };

        // Use write_type_name if available to include generic parameters,
        // otherwise fall back to type_identifier
        static_self.write_type_name(f, crate::TypeNameOpts::default())
    }
}

impl fmt::Debug for Shape {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // NOTE:
        // This dummy destructuring is present to ensure that if fields are added,
        // developers will get a compiler error in this function, reminding them
        // to carefully consider whether it should be shown when debug formatting.
        let Self {
            id: _,      // omit by default
            decl_id: _, // omit by default (opaque, unstable)
            layout: _,
            vtable: _,   // omit by default
            type_ops: _, // omit by default (per-T operations)
            marker_traits: _,
            ty: _,
            def: _,
            type_identifier: _,
            module_path: _,   // omit by default (for code generation)
            source_file: _,   // omit by default (for debugging)
            source_line: _,   // omit by default (for debugging)
            source_column: _, // omit by default (for debugging)
            type_params: _,
            doc: _,
            attributes: _,
            type_tag: _,
            inner: _,
            builder_shape: _,
            type_name: _,
            #[cfg(feature = "alloc")]
                proxy: _,
            #[cfg(feature = "alloc")]
                format_proxies: _,
            variance: _,
            flags: _,
            tag: _,
            content: _,
            rename: _,
        } = self;

        if f.alternate() {
            f.debug_struct("Shape")
                .field("id", &self.id)
                .field("layout", &format_args!("{:?}", self.layout))
                .field("vtable", &format_args!("VTable {{ .. }}"))
                .field("marker_traits", &format_args!("{:?}", self.marker_traits))
                .field("ty", &self.ty)
                .field("def", &self.def)
                .field("type_identifier", &self.type_identifier)
                .field("type_params", &self.type_params)
                .field("doc", &self.doc)
                .field("attributes", &self.attributes)
                .field("type_tag", &self.type_tag)
                .field("inner", &self.inner)
                .finish()
        } else {
            let mut debug_struct = f.debug_struct("Shape");

            macro_rules! field {
                ( $field:literal, $( $fmt_args:tt )* ) => {{
                    debug_struct.field($field, &format_args!($($fmt_args)*));
                }};
            }

            field!("type_identifier", "{:?}", self.type_identifier);

            if !self.type_params.is_empty() {
                // Use `[]` to indicate empty `type_params` (a real empty slice),
                // and `«(...)»` to show custom-formatted parameter sets when present.
                // Avoids visual conflict with array types like `[T; N]` in other fields.
                field!("type_params", "{}", {
                    struct TypeParams<'shape>(&'shape [TypeParam]);
                    impl core::fmt::Display for TypeParams<'_> {
                        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                            let mut iter = self.0.iter();
                            if let Some(first) = iter.next() {
                                write!(f, "«({}: {}", first.name, first.shape)?;
                                for next in iter {
                                    write!(f, ", {}: {}", next.name, next.shape)?;
                                }
                                write!(f, ")»")?;
                            } else {
                                write!(f, "[]")?;
                            }
                            Ok(())
                        }
                    }
                    TypeParams(self.type_params)
                });
            }

            if let Some(type_tag) = self.type_tag {
                field!("type_tag", "{:?}", type_tag);
            }

            if !self.attributes.is_empty() {
                field!("attributes", "{:?}", self.attributes);
            }

            // Omit the `inner` field if this shape is not a transparent wrapper.
            if let Some(inner) = self.inner {
                field!("inner", "{:?}", inner);
            }

            // Uses `Display` to potentially format with shorthand syntax.
            field!("ty", "{}", self.ty);

            // For sized layouts, display size and alignment in shorthand.
            // NOTE: If you wish to display the bitshift for alignment, please open an issue.
            if let ShapeLayout::Sized(layout) = self.layout {
                field!(
                    "layout",
                    "Sized(«{} align {}»)",
                    layout.size(),
                    layout.align()
                );
            } else {
                field!("layout", "{:?}", self.layout);
            }

            // If `def` is `Undefined`, the information in `ty` would be more useful.
            if !matches!(self.def, Def::Undefined) {
                field!("def", "{:?}", self.def);
            }

            if !self.doc.is_empty() {
                // TODO: Should these be called "strings"? Because `#[doc]` can contain newlines.
                field!("doc", "«{} lines»", self.doc.len());
            }

            debug_struct.finish_non_exhaustive()
        }
    }
}
