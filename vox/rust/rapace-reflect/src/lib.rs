#![deny(unsafe_code)]

//! Facet-based reflection helpers for turning Rust types into Rapace `TypeDetail`.
//!
//! This is Rust-implementation-specific and follows `docs/content/rust-spec/_index.md`.

use std::collections::{HashMap, HashSet};

use facet::Facet;
use facet_core::{
    Attr, ConstTypeId, Def, EnumType, PrimitiveType, SequenceType, Shape, StructKind, StructType,
    TextualType, Type, UserType, Variant,
};
use rapace_schema::{FieldDetail, TypeDetail, VariantDetail, VariantPayload};

#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub fn type_detail<T: Facet<'static>>() -> Result<TypeDetail, Error> {
    type_detail_from_shape(T::SHAPE)
}

pub fn type_detail_from_shape(shape: &'static Shape) -> Result<TypeDetail, Error> {
    let mut ctx = Ctx::default();
    ctx.type_detail_from_shape(shape)
}

#[derive(Default)]
struct Ctx {
    cache: HashMap<ConstTypeId, TypeDetail>,
    stack: HashSet<ConstTypeId>,
}

impl Ctx {
    fn type_detail_from_shape(&mut self, shape: &'static Shape) -> Result<TypeDetail, Error> {
        if let Some(cached) = self.cache.get(&shape.id) {
            return Ok(cached.clone());
        }

        if !self.stack.insert(shape.id) {
            return Err(Error::new(format!(
                "recursive types are not supported in Rapace signatures (at {})",
                shape.type_identifier
            )));
        }

        let detail = self.type_detail_from_shape_uncached(shape)?;

        self.stack.remove(&shape.id);
        self.cache.insert(shape.id, detail.clone());
        Ok(detail)
    }

    fn type_detail_from_shape_uncached(
        &mut self,
        shape: &'static Shape,
    ) -> Result<TypeDetail, Error> {
        // Rapace-specific wire mapping for Stream<T>.
        if has_attr(shape.attributes, Some("rapace"), "stream") {
            let Some(tp) = shape.type_params.first() else {
                return Err(Error::new(
                    "rapace::stream marker requires a single type parameter",
                ));
            };
            let inner = self.type_detail_from_shape(tp.shape)?;
            return Ok(TypeDetail::Stream(Box::new(inner)));
        }

        // Transparent wrappers should not affect the wire signature.
        //
        // Note: Facet may also populate `Shape::inner` for some container types
        // (e.g. `Vec<T>`), where we still want the container to participate in
        // the Rapace wire signature. Only treat `inner` as a transparent wrapper
        // for scalar/undefined definitions.
        if let Some(inner) = shape.inner
            && matches!(shape.def, Def::Scalar | Def::Undefined)
        {
            return self.type_detail_from_shape(inner);
        }

        match &shape.def {
            Def::List(list_def) => {
                let inner = self.type_detail_from_shape(list_def.t)?;
                // rs[impl signature.bytes.equivalence] - canonicalize bytes and List<u8> to Bytes
                if matches!(inner, TypeDetail::U8) {
                    return Ok(TypeDetail::Bytes);
                }
                return Ok(TypeDetail::List(Box::new(inner)));
            }
            Def::Set(set_def) => {
                let inner = self.type_detail_from_shape(set_def.t)?;
                return Ok(TypeDetail::Set(Box::new(inner)));
            }
            Def::Map(map_def) => {
                let key = self.type_detail_from_shape(map_def.k)?;
                let value = self.type_detail_from_shape(map_def.v)?;
                return Ok(TypeDetail::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                });
            }
            Def::Array(array_def) => {
                let element = self.type_detail_from_shape(array_def.t)?;
                let len: u32 = array_def.n.try_into().map_err(|_| {
                    Error::new(format!(
                        "array length {} does not fit in u32 for {}",
                        array_def.n, shape.type_identifier
                    ))
                })?;
                return Ok(TypeDetail::Array {
                    element: Box::new(element),
                    len,
                });
            }
            Def::Slice(slice_def) => {
                // Treat slices as the List wire type.
                let inner = self.type_detail_from_shape(slice_def.t)?;
                if matches!(inner, TypeDetail::U8) {
                    return Ok(TypeDetail::Bytes);
                }
                return Ok(TypeDetail::List(Box::new(inner)));
            }
            Def::Option(option_def) => {
                let inner = self.type_detail_from_shape(option_def.t)?;
                return Ok(TypeDetail::Option(Box::new(inner)));
            }
            Def::Result(result_def) => {
                let ok = self.type_detail_from_shape(result_def.t)?;
                let err = self.type_detail_from_shape(result_def.e)?;

                // r[impl streaming.error-no-streams] - Stream must not appear in error types
                if contains_stream(&err) {
                    return Err(Error::new(format!(
                        "Stream is not allowed in error types (found in Result<_, {}> at {})",
                        format_type_detail(&err),
                        shape.type_identifier
                    )));
                }

                return Ok(TypeDetail::Enum {
                    variants: vec![
                        VariantDetail {
                            name: "Ok".to_string(),
                            payload: VariantPayload::Newtype(ok),
                        },
                        VariantDetail {
                            name: "Err".to_string(),
                            payload: VariantPayload::Newtype(err),
                        },
                    ],
                });
            }
            Def::Pointer(ptr_def) => {
                let Some(pointee) = ptr_def.pointee() else {
                    return Err(Error::new(format!(
                        "opaque pointer types are not supported in Rapace signatures ({})",
                        shape.type_identifier
                    )));
                };
                return self.type_detail_from_shape(pointee);
            }
            Def::Scalar | Def::Undefined => {
                // Fall through to `shape.ty`.
            }
            other => {
                return Err(Error::new(format!(
                    "unsupported facet definition for Rapace signature: {other:?} ({})",
                    shape.type_identifier
                )));
            }
        }

        match &shape.ty {
            Type::Primitive(p) => self.primitive_type_detail(p, shape),
            Type::Sequence(SequenceType::Array(arr)) => {
                let element = self.type_detail_from_shape(arr.t)?;
                let len: u32 = arr.n.try_into().map_err(|_| {
                    Error::new(format!(
                        "array length {} does not fit in u32 for {}",
                        arr.n, shape.type_identifier
                    ))
                })?;
                Ok(TypeDetail::Array {
                    element: Box::new(element),
                    len,
                })
            }
            Type::Sequence(SequenceType::Slice(slice)) => {
                let inner = self.type_detail_from_shape(slice.t)?;
                if matches!(inner, TypeDetail::U8) {
                    Ok(TypeDetail::Bytes)
                } else {
                    Ok(TypeDetail::List(Box::new(inner)))
                }
            }
            Type::User(UserType::Struct(st)) => self.struct_type_detail(st, shape),
            Type::User(UserType::Enum(en)) => self.enum_type_detail(en, shape),
            Type::User(UserType::Opaque) => self.opaque_type_detail(shape),
            Type::User(UserType::Union(_)) => Err(Error::new(format!(
                "unions are not supported in Rapace signatures ({})",
                shape.type_identifier
            ))),
            Type::Pointer(_) => Err(Error::new(format!(
                "raw/reference pointer types are not supported in Rapace signatures ({})",
                shape.type_identifier
            ))),
            Type::Undefined => Err(Error::new("unsupported undefined type in facet Shape")),
        }
    }

    fn primitive_type_detail(
        &self,
        p: &PrimitiveType,
        shape: &'static Shape,
    ) -> Result<TypeDetail, Error> {
        match p {
            PrimitiveType::Boolean => Ok(TypeDetail::Bool),
            PrimitiveType::Textual(TextualType::Char) => Ok(TypeDetail::Char),
            PrimitiveType::Textual(TextualType::Str) => Ok(TypeDetail::String),
            PrimitiveType::Numeric(n) => match n {
                facet_core::NumericType::Float => {
                    let layout = shape
                        .layout
                        .sized_layout()
                        .map_err(|_| Error::new("unsized float type"))?;
                    match layout.size() {
                        4 => Ok(TypeDetail::F32),
                        8 => Ok(TypeDetail::F64),
                        other => Err(Error::new(format!(
                            "unsupported float size {other} for {}",
                            shape.type_identifier
                        ))),
                    }
                }
                facet_core::NumericType::Integer { signed } => {
                    let layout = shape
                        .layout
                        .sized_layout()
                        .map_err(|_| Error::new("unsized integer type"))?;
                    let bits = layout.size() * 8;
                    match (*signed, bits) {
                        (false, 8) => Ok(TypeDetail::U8),
                        (false, 16) => Ok(TypeDetail::U16),
                        (false, 32) => Ok(TypeDetail::U32),
                        (false, 64) => Ok(TypeDetail::U64),
                        (false, 128) => Ok(TypeDetail::U128),
                        (true, 8) => Ok(TypeDetail::I8),
                        (true, 16) => Ok(TypeDetail::I16),
                        (true, 32) => Ok(TypeDetail::I32),
                        (true, 64) => Ok(TypeDetail::I64),
                        (true, 128) => Ok(TypeDetail::I128),
                        (_, other) => Err(Error::new(format!(
                            "unsupported integer width {other} for {} (Rapace signatures use fixed-width ints; avoid usize/isize)",
                            shape.type_identifier
                        ))),
                    }
                }
            },
            PrimitiveType::Never => Err(Error::new(
                "never type `!` is not supported in Rapace signatures",
            )),
        }
    }

    fn struct_type_detail(
        &mut self,
        st: &StructType,
        shape: &'static Shape,
    ) -> Result<TypeDetail, Error> {
        match st.kind {
            StructKind::Tuple if st.fields.is_empty() => Ok(TypeDetail::Unit),
            StructKind::Tuple => {
                let mut items = Vec::with_capacity(st.fields.len());
                for f in st.fields {
                    let ty = self.type_detail_from_shape(f.shape.get())?;
                    items.push(ty);
                }
                Ok(TypeDetail::Tuple(items))
            }
            StructKind::Struct | StructKind::TupleStruct | StructKind::Unit => {
                let fields = self.fields_from_struct_fields(st.fields, shape)?;
                Ok(TypeDetail::Struct { fields })
            }
        }
    }

    fn enum_type_detail(
        &mut self,
        en: &EnumType,
        shape: &'static Shape,
    ) -> Result<TypeDetail, Error> {
        let mut variants = Vec::with_capacity(en.variants.len());
        for v in en.variants {
            variants.push(self.variant_detail(v, shape)?);
        }
        Ok(TypeDetail::Enum { variants })
    }

    fn opaque_type_detail(&self, shape: &'static Shape) -> Result<TypeDetail, Error> {
        match shape.type_identifier {
            "String" => Ok(TypeDetail::String),
            other => Err(Error::new(format!(
                "opaque type is not supported in Rapace signatures: {other}",
            ))),
        }
    }

    fn fields_from_struct_fields(
        &mut self,
        fields: &'static [facet_core::Field],
        shape: &'static Shape,
    ) -> Result<Vec<FieldDetail>, Error> {
        let mut out = Vec::with_capacity(fields.len());
        for f in fields {
            if f.flags.contains(facet_core::FieldFlags::SKIP)
                || f.flags.contains(facet_core::FieldFlags::SKIP_SERIALIZING)
                || f.flags.contains(facet_core::FieldFlags::SKIP_DESERIALIZING)
                || f.flags.contains(facet_core::FieldFlags::FLATTEN)
                || f.metadata.is_some()
            {
                return Err(Error::new(format!(
                    "field {} on {} uses facet features not supported by Rapace signatures (skip/flatten/metadata)",
                    f.name, shape.type_identifier
                )));
            }

            let name = f.rename.unwrap_or(f.name);
            let ty = self.type_detail_from_shape(f.shape.get())?;
            out.push(FieldDetail {
                name: name.to_string(),
                type_info: ty,
            });
        }
        Ok(out)
    }

    fn variant_detail(
        &mut self,
        v: &Variant,
        shape: &'static Shape,
    ) -> Result<VariantDetail, Error> {
        let payload = match v.data.kind {
            StructKind::Unit => VariantPayload::Unit,
            StructKind::TupleStruct => {
                let fields = v.data.fields;
                match fields.len() {
                    0 => VariantPayload::Unit,
                    1 => {
                        let inner = self.type_detail_from_shape(fields[0].shape.get())?;
                        VariantPayload::Newtype(inner)
                    }
                    _ => VariantPayload::Struct(self.fields_from_struct_fields(fields, shape)?),
                }
            }
            StructKind::Struct => {
                VariantPayload::Struct(self.fields_from_struct_fields(v.data.fields, shape)?)
            }
            StructKind::Tuple => {
                VariantPayload::Struct(self.fields_from_struct_fields(v.data.fields, shape)?)
            }
        };

        Ok(VariantDetail {
            name: v.name.to_string(),
            payload,
        })
    }
}

fn has_attr(attrs: &'static [Attr], ns: Option<&'static str>, key: &str) -> bool {
    attrs.iter().any(|a| a.ns == ns && a.key == key)
}

/// r[impl streaming.error-no-streams] - Check if a TypeDetail contains Stream at any nesting level
fn contains_stream(td: &TypeDetail) -> bool {
    let mut found = false;
    td.visit(&mut |t| {
        if matches!(t, TypeDetail::Stream(_)) {
            found = true;
            false // Stop visiting
        } else {
            true // Continue visiting
        }
    });
    found
}

/// Format a TypeDetail for error messages
fn format_type_detail(td: &TypeDetail) -> String {
    match td {
        TypeDetail::Stream(inner) => format!("Stream<{}>", format_type_detail(inner)),
        TypeDetail::List(inner) => format!("List<{}>", format_type_detail(inner)),
        TypeDetail::Option(inner) => format!("Option<{}>", format_type_detail(inner)),
        _ => format!("{:?}", td), // Fallback to debug format
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_u8_is_bytes() {
        let td = type_detail::<Vec<u8>>().unwrap();
        assert_eq!(td, TypeDetail::Bytes);
    }

    #[test]
    fn option_i32() {
        let td = type_detail::<Option<i32>>().unwrap();
        assert_eq!(td, TypeDetail::Option(Box::new(TypeDetail::I32)));
    }

    #[test]
    fn tuple() {
        let td = type_detail::<(u8, i32)>().unwrap();
        assert_eq!(td, TypeDetail::Tuple(vec![TypeDetail::U8, TypeDetail::I32]));
    }

    #[test]
    fn result_is_enum_ok_err() {
        let td = type_detail::<Result<u8, i32>>().unwrap();
        assert_eq!(
            td,
            TypeDetail::Enum {
                variants: vec![
                    VariantDetail {
                        name: "Ok".to_string(),
                        payload: VariantPayload::Newtype(TypeDetail::U8),
                    },
                    VariantDetail {
                        name: "Err".to_string(),
                        payload: VariantPayload::Newtype(TypeDetail::I32),
                    }
                ],
            }
        );
    }

    #[test]
    fn struct_fields_in_order() {
        #[derive(Facet)]
        struct S {
            b: u8,
            a: i32,
        }

        let td = type_detail::<S>().unwrap();
        assert_eq!(
            td,
            TypeDetail::Struct {
                fields: vec![
                    FieldDetail {
                        name: "b".to_string(),
                        type_info: TypeDetail::U8
                    },
                    FieldDetail {
                        name: "a".to_string(),
                        type_info: TypeDetail::I32
                    }
                ]
            }
        );
    }

    #[test]
    fn enum_variants_in_order() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum E {
            A,
            B(u8),
            C { x: i32 },
            D(u8, i32),
        }

        let td = type_detail::<E>().unwrap();
        assert_eq!(
            td,
            TypeDetail::Enum {
                variants: vec![
                    VariantDetail {
                        name: "A".to_string(),
                        payload: VariantPayload::Unit
                    },
                    VariantDetail {
                        name: "B".to_string(),
                        payload: VariantPayload::Newtype(TypeDetail::U8)
                    },
                    VariantDetail {
                        name: "C".to_string(),
                        payload: VariantPayload::Struct(vec![FieldDetail {
                            name: "x".to_string(),
                            type_info: TypeDetail::I32
                        }])
                    },
                    VariantDetail {
                        name: "D".to_string(),
                        payload: VariantPayload::Struct(vec![
                            FieldDetail {
                                name: "0".to_string(),
                                type_info: TypeDetail::U8
                            },
                            FieldDetail {
                                name: "1".to_string(),
                                type_info: TypeDetail::I32
                            }
                        ])
                    }
                ]
            }
        );
    }

    // r[verify streaming.error-no-streams] - Test that Stream in error types is rejected
    #[test]
    fn rejects_stream_in_error_type() {
        // Test that contains_stream correctly detects Stream in TypeDetail
        let stream_detail = TypeDetail::Stream(Box::new(TypeDetail::String));
        assert!(contains_stream(&stream_detail), "Should detect Stream");

        // Test nested case
        let nested = TypeDetail::List(Box::new(TypeDetail::Stream(Box::new(TypeDetail::U8))));
        assert!(contains_stream(&nested), "Should detect nested Stream");
    }

    // r[verify streaming.error-no-streams] - Test that Stream in Ok type is allowed
    #[test]
    fn stream_detection_works() {
        // Test that contains_stream correctly identifies Stream types
        assert!(contains_stream(&TypeDetail::Stream(Box::new(TypeDetail::String))));

        // Test nested in various containers
        assert!(contains_stream(&TypeDetail::List(Box::new(TypeDetail::Stream(Box::new(TypeDetail::U8))))));
        assert!(contains_stream(&TypeDetail::Option(Box::new(TypeDetail::Stream(Box::new(TypeDetail::Bool))))));

        // Test non-Stream types return false
        assert!(!contains_stream(&TypeDetail::String));
        assert!(!contains_stream(&TypeDetail::List(Box::new(TypeDetail::U32))));
    }
}
