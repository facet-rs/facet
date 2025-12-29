use crate::types::{
    EnumReprLike, FunctionAbi, KnownPointer, PointerFlags, PrimitiveType, ReprLike, ShapeLayout,
    StructKindLike,
};
use facet::Facet;
use facet_core::{
    ArrayDef, Attr, Def, EnumType, Field, FunctionPointerDef, ListDef, MapDef, NdArrayDef,
    OptionDef, PointerDef, PointerType, ResultDef, SequenceType, SetDef, Shape, SliceDef,
    StructType, Type, UserType, Variant,
};
use facet_reflect::Peek;

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ShapeLike {
    pub layout: ShapeLayout,
    pub ty: TypeLike,
    pub def: DefLike,
    pub type_identifier: String,
    pub type_params: Vec<TypeParamLike>,
    pub doc: Vec<String>,
    pub attributes: Vec<AttrLike>,
    pub type_tag: Option<String>,
    #[facet(recursive_type)]
    pub inner: Option<Box<ShapeLike>>,
}

impl From<&Shape> for ShapeLike {
    fn from(shape: &Shape) -> Self {
        Self {
            layout: match shape.layout {
                facet_core::ShapeLayout::Sized(l) => ShapeLayout::Sized(l.into()),
                facet_core::ShapeLayout::Unsized => ShapeLayout::Unsized,
            },
            ty: (&shape.ty).into(),
            def: (&shape.def).into(),
            type_identifier: shape.type_identifier.to_string(),
            type_params: shape.type_params.iter().map(|item| (item).into()).collect(),
            doc: shape.doc.iter().map(|s| s.to_string()).collect(),
            attributes: shape.attributes.iter().map(|item| (item).into()).collect(),
            type_tag: shape.type_tag.map(|s| s.to_string()),
            inner: shape.inner.map(|s| Box::new((s).into())),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct TypeParamLike {
    pub name: String,
    #[facet(recursive_type)]
    pub shape: Box<ShapeLike>,
}

impl From<&facet_core::TypeParam> for TypeParamLike {
    fn from(tp: &facet_core::TypeParam) -> Self {
        Self {
            name: tp.name.to_string(),
            shape: Box::new((tp.shape).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct AttrLike {
    pub ns: Option<String>,
    pub key: String,
    pub data: Vec<u8>,
    #[facet(recursive_type)]
    pub shape: Box<ShapeLike>,
}

impl From<&Attr> for AttrLike {
    fn from(attr: &Attr) -> Self {
        let ptr = attr.data.ptr();
        let shape = attr.data.shape;
        let peek = unsafe { Peek::unchecked_new(ptr, shape) };
        let data = facet_postcard_legacy::ptr_to_vec(peek).unwrap();
        Self {
            ns: attr.ns.map(|s| s.to_string()),
            key: attr.key.to_string(),
            data,
            shape: Box::new(shape.into()),
        }
    }
}

impl AttrLike {
    pub fn parse_data<T: Facet<'static>>(
        &self,
    ) -> Result<T, facet_postcard_legacy::DeserializeError> {
        facet_postcard_legacy::from_slice(&self.data)
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum TypeLike {
    Undefined,
    Primitive(PrimitiveType),
    Sequence(SequenceTypeLike),
    User(UserTypeLike),
    Pointer(PointerTypeLike),
}

impl From<&Type> for TypeLike {
    fn from(ty: &Type) -> Self {
        match ty {
            Type::Undefined => TypeLike::Undefined,
            Type::Primitive(p) => TypeLike::Primitive(match p {
                facet_core::PrimitiveType::Boolean => PrimitiveType::Boolean,
                facet_core::PrimitiveType::Numeric(n) => PrimitiveType::Numeric(match n {
                    facet_core::NumericType::Integer { signed } => {
                        crate::types::NumericType::Integer { signed: *signed }
                    }
                    facet_core::NumericType::Float => crate::types::NumericType::Float,
                }),
                facet_core::PrimitiveType::Textual(t) => PrimitiveType::Textual(match t {
                    facet_core::TextualType::Char => crate::types::TextualType::Char,
                    facet_core::TextualType::Str => crate::types::TextualType::Str,
                }),
                facet_core::PrimitiveType::Never => PrimitiveType::Never,
            }),
            Type::Sequence(s) => TypeLike::Sequence(s.into()),
            Type::User(u) => TypeLike::User(u.into()),
            Type::Pointer(p) => TypeLike::Pointer(p.into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum SequenceTypeLike {
    Array(ArrayTypeLike),
    Slice(SliceTypeLike),
}

impl From<&SequenceType> for SequenceTypeLike {
    fn from(seq: &SequenceType) -> Self {
        match seq {
            SequenceType::Array(a) => SequenceTypeLike::Array(a.into()),
            SequenceType::Slice(s) => SequenceTypeLike::Slice(s.into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ArrayTypeLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
    pub n: u64,
}

impl From<&facet_core::ArrayType> for ArrayTypeLike {
    fn from(a: &facet_core::ArrayType) -> Self {
        Self {
            t: Box::new((a.t).into()),
            n: a.n as u64,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct SliceTypeLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
}

impl From<&facet_core::SliceType> for SliceTypeLike {
    fn from(s: &facet_core::SliceType) -> Self {
        Self {
            t: Box::new((s.t).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum UserTypeLike {
    Struct(StructTypeLike),
    Enum(EnumTypeLike),
    Union(UnionTypeLike),
    Opaque,
}

impl From<&UserType> for UserTypeLike {
    fn from(user: &UserType) -> Self {
        match user {
            UserType::Struct(s) => UserTypeLike::Struct(s.into()),
            UserType::Enum(e) => UserTypeLike::Enum(e.into()),
            UserType::Union(u) => UserTypeLike::Union(u.into()),
            UserType::Opaque => UserTypeLike::Opaque,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct StructTypeLike {
    pub repr: ReprLike,
    pub kind: StructKindLike,
    pub fields: Vec<FieldLike>,
}

impl From<&StructType> for StructTypeLike {
    fn from(s: &StructType) -> Self {
        Self {
            repr: s.repr.into(),
            kind: s.kind.into(),
            fields: s.fields.iter().map(|item| (item).into()).collect(),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct EnumTypeLike {
    pub repr: ReprLike,
    pub enum_repr: EnumReprLike,
    pub variants: Vec<VariantLike>,
}

impl From<&EnumType> for EnumTypeLike {
    fn from(e: &EnumType) -> Self {
        Self {
            repr: e.repr.into(),
            enum_repr: e.enum_repr.into(),
            variants: e.variants.iter().map(|item| (item).into()).collect(),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct UnionTypeLike {
    pub repr: ReprLike,
    pub fields: Vec<FieldLike>,
}

impl From<&facet_core::UnionType> for UnionTypeLike {
    fn from(u: &facet_core::UnionType) -> Self {
        Self {
            repr: u.repr.into(),
            fields: u.fields.iter().map(|item| (item).into()).collect(),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct FieldLike {
    pub name: String,
    #[facet(recursive_type)]
    pub shape: Box<ShapeLike>,
    pub offset: usize,
    pub attributes: Vec<AttrLike>,
    pub doc: Vec<String>,
}

impl From<&Field> for FieldLike {
    fn from(f: &Field) -> Self {
        Self {
            name: f.name.to_string(),
            shape: Box::new(f.shape().into()),
            offset: f.offset,
            attributes: f.attributes.iter().map(|x| x.into()).collect(),
            doc: f.doc.iter().map(|x| x.to_string()).collect(),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct VariantLike {
    pub name: String,
    pub discriminant: Option<i64>,
    pub attributes: Vec<AttrLike>,
    pub data: StructTypeLike,
    pub doc: Vec<String>,
}

impl From<&Variant> for VariantLike {
    fn from(v: &Variant) -> Self {
        Self {
            name: v.name.to_string(),
            discriminant: v.discriminant,
            attributes: v.attributes.iter().map(|item| (item).into()).collect(),
            data: (&v.data).into(),
            doc: v.doc.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum PointerTypeLike {
    Reference(ReferenceTypeLike),
    Raw(RawTypeLike),
    Function(FunctionPointerDefLike),
}

impl From<&PointerType> for PointerTypeLike {
    fn from(p: &PointerType) -> Self {
        match p {
            PointerType::Reference(r) => PointerTypeLike::Reference(r.into()),
            PointerType::Raw(r) => PointerTypeLike::Raw(r.into()),
            PointerType::Function(f) => PointerTypeLike::Function(f.into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ReferenceTypeLike {
    pub mutable: bool,
    #[facet(recursive_type)]
    pub target: Box<ShapeLike>,
}

impl From<&facet_core::ValuePointerType> for ReferenceTypeLike {
    fn from(r: &facet_core::ValuePointerType) -> Self {
        Self {
            mutable: r.mutable,
            target: Box::new((r.target).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct RawTypeLike {
    pub mutable: bool,
    #[facet(recursive_type)]
    pub target: Box<ShapeLike>,
}

impl From<&facet_core::ValuePointerType> for RawTypeLike {
    fn from(r: &facet_core::ValuePointerType) -> Self {
        Self {
            mutable: r.mutable,
            target: Box::new((r.target).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct FunctionPointerDefLike {
    pub abi: FunctionAbi,
    #[facet(recursive_type)]
    pub parameters: Vec<ShapeLike>,
    #[facet(recursive_type)]
    pub return_type: Box<ShapeLike>,
}

impl From<&FunctionPointerDef> for FunctionPointerDefLike {
    fn from(f: &FunctionPointerDef) -> Self {
        Self {
            abi: match f.abi {
                facet_core::FunctionAbi::C => FunctionAbi::C,
                facet_core::FunctionAbi::Rust => FunctionAbi::Rust,
                facet_core::FunctionAbi::Unknown => FunctionAbi::Unknown,
            },
            parameters: f.parameters.iter().map(|&s| s.into()).collect::<Vec<_>>(),
            return_type: Box::new((f.return_type).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum DefLike {
    Undefined,
    Scalar,
    Map(MapDefLike),
    Set(SetDefLike),
    List(ListDefLike),
    Array(ArrayDefLike),
    NdArray(NdArrayDefLike),
    Slice(SliceDefLike),
    Option(OptionDefLike),
    Result(ResultDefLike),
    Pointer(PointerDefLike),
    DynamicValue,
}

impl From<&Def> for DefLike {
    fn from(def: &Def) -> Self {
        match def {
            Def::Undefined => DefLike::Undefined,
            Def::Scalar => DefLike::Scalar,
            Def::Map(m) => DefLike::Map(m.into()),
            Def::Set(s) => DefLike::Set(s.into()),
            Def::List(l) => DefLike::List(l.into()),
            Def::Array(a) => DefLike::Array(a.into()),
            Def::NdArray(n) => DefLike::NdArray(n.into()),
            Def::Slice(s) => DefLike::Slice(s.into()),
            Def::Option(o) => DefLike::Option(o.into()),
            Def::Result(r) => DefLike::Result(r.into()),
            Def::Pointer(p) => DefLike::Pointer(p.into()),
            Def::DynamicValue(_d) => DefLike::DynamicValue,
            _ => DefLike::Undefined,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct MapDefLike {
    #[facet(recursive_type)]
    pub k: Box<ShapeLike>,
    #[facet(recursive_type)]
    pub v: Box<ShapeLike>,
}

impl From<&MapDef> for MapDefLike {
    fn from(m: &MapDef) -> Self {
        Self {
            k: Box::new((m.k).into()),
            v: Box::new((m.v).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct SetDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
}

impl From<&SetDef> for SetDefLike {
    fn from(s: &SetDef) -> Self {
        Self {
            t: Box::new((s.t).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ListDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
}

impl From<&ListDef> for ListDefLike {
    fn from(l: &ListDef) -> Self {
        Self {
            t: Box::new((l.t).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ArrayDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
    pub n: u64,
}

impl From<&ArrayDef> for ArrayDefLike {
    fn from(a: &ArrayDef) -> Self {
        Self {
            t: Box::new((a.t).into()),
            n: a.n as u64,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct NdArrayDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
}

impl From<&NdArrayDef> for NdArrayDefLike {
    fn from(n: &NdArrayDef) -> Self {
        Self {
            t: Box::new((n.t).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct SliceDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
}

impl From<&SliceDef> for SliceDefLike {
    fn from(s: &SliceDef) -> Self {
        Self {
            t: Box::new((s.t).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct OptionDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
}

impl From<&OptionDef> for OptionDefLike {
    fn from(o: &OptionDef) -> Self {
        Self {
            t: Box::new((o.t).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ResultDefLike {
    #[facet(recursive_type)]
    pub t: Box<ShapeLike>,
    #[facet(recursive_type)]
    pub e: Box<ShapeLike>,
}

impl From<&ResultDef> for ResultDefLike {
    fn from(r: &ResultDef) -> Self {
        Self {
            t: Box::new((r.t).into()),
            e: Box::new((r.e).into()),
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct PointerDefLike {
    #[facet(recursive_type)]
    pub pointee: Option<Box<ShapeLike>>,
    #[facet(recursive_type)]
    pub weak: Option<Box<ShapeLike>>,
    #[facet(recursive_type)]
    pub strong: Option<Box<ShapeLike>>,
    pub flags: PointerFlags,
    pub known: Option<KnownPointer>,
}

impl From<&PointerDef> for PointerDefLike {
    fn from(p: &PointerDef) -> Self {
        Self {
            pointee: p.pointee.map(|s| Box::new((s).into())),
            weak: p.weak.map(|f| Box::new((f()).into())),
            strong: p.strong.map(|s| Box::new((s).into())),
            flags: PointerFlags::from_bits_truncate(p.flags.bits()),
            known: p.known.map(|k| match k {
                facet_core::KnownPointer::Box => KnownPointer::Box,
                facet_core::KnownPointer::Rc => KnownPointer::Rc,
                facet_core::KnownPointer::RcWeak => KnownPointer::RcWeak,
                facet_core::KnownPointer::Arc => KnownPointer::Arc,
                facet_core::KnownPointer::ArcWeak => KnownPointer::ArcWeak,
                facet_core::KnownPointer::Cow => KnownPointer::Cow,
                facet_core::KnownPointer::Pin => KnownPointer::Pin,
                facet_core::KnownPointer::Cell => KnownPointer::Cell,
                facet_core::KnownPointer::RefCell => KnownPointer::RefCell,
                facet_core::KnownPointer::OnceCell => KnownPointer::OnceCell,
                facet_core::KnownPointer::Mutex => KnownPointer::Mutex,
                facet_core::KnownPointer::RwLock => KnownPointer::RwLock,
                facet_core::KnownPointer::NonNull => KnownPointer::NonNull,
                facet_core::KnownPointer::SharedReference => KnownPointer::SharedReference,
                facet_core::KnownPointer::ExclusiveReference => KnownPointer::ExclusiveReference,
            }),
        }
    }
}
