//! Facet implementation for Value, enabling deserialization from any format.

use core::ptr::NonNull;

use facet_core::{
    ConstTypeId, Def, DynValueKind, DynamicValueDef, DynamicValueVTable, Facet, MarkerTraits,
    PtrConst, PtrMut, PtrUninit, Shape, ShapeLayout, Type, TypeNameOpts, UserType, ValueVTable,
};

use crate::{VArray, VBytes, VNumber, VObject, VString, Value};

// ============================================================================
// Scalar setters
// ============================================================================

unsafe fn dyn_set_null(dst: PtrUninit<'_>) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(Value::NULL);
    }
}

unsafe fn dyn_set_bool(dst: PtrUninit<'_>, value: bool) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(Value::from(value));
    }
}

unsafe fn dyn_set_i64(dst: PtrUninit<'_>, value: i64) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VNumber::from_i64(value).into_value());
    }
}

unsafe fn dyn_set_u64(dst: PtrUninit<'_>, value: u64) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VNumber::from_u64(value).into_value());
    }
}

unsafe fn dyn_set_f64(dst: PtrUninit<'_>, value: f64) -> bool {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        match VNumber::from_f64(value) {
            Some(num) => {
                ptr.write(num.into_value());
                true
            }
            None => {
                // NaN or infinity - write null as fallback and return false
                ptr.write(Value::NULL);
                false
            }
        }
    }
}

unsafe fn dyn_set_str(dst: PtrUninit<'_>, value: &str) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VString::new(value).into_value());
    }
}

unsafe fn dyn_set_bytes(dst: PtrUninit<'_>, value: &[u8]) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VBytes::new(value).into_value());
    }
}

// ============================================================================
// Array operations
// ============================================================================

unsafe fn dyn_begin_array(dst: PtrUninit<'_>) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VArray::new().into_value());
    }
}

unsafe fn dyn_push_array_element(array: PtrMut<'_>, element: PtrMut<'_>) {
    unsafe {
        let array_ptr = array.as_mut_byte_ptr() as *mut Value;
        let element_ptr = element.as_mut_byte_ptr() as *mut Value;

        // Read the element (moving it out)
        let element_value = element_ptr.read();

        // Get the array and push
        let array_value = &mut *array_ptr;
        if let Some(arr) = array_value.as_array_mut() {
            arr.push(element_value);
        }
    }
}

// ============================================================================
// Object operations
// ============================================================================

unsafe fn dyn_begin_object(dst: PtrUninit<'_>) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VObject::new().into_value());
    }
}

unsafe fn dyn_insert_object_entry(object: PtrMut<'_>, key: &str, value: PtrMut<'_>) {
    unsafe {
        let object_ptr = object.as_mut_byte_ptr() as *mut Value;
        let value_ptr = value.as_mut_byte_ptr() as *mut Value;

        // Read the value (moving it out)
        let entry_value = value_ptr.read();

        // Get the object and insert
        let object_value = &mut *object_ptr;
        if let Some(obj) = object_value.as_object_mut() {
            obj.insert(key, entry_value);
        }
    }
}

// ============================================================================
// Read operations
// ============================================================================

unsafe fn dyn_get_kind(value: PtrConst<'_>) -> DynValueKind {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        let v = &*ptr;
        match v.value_type() {
            crate::ValueType::Null => DynValueKind::Null,
            crate::ValueType::Bool => DynValueKind::Bool,
            crate::ValueType::Number => DynValueKind::Number,
            crate::ValueType::String => DynValueKind::String,
            crate::ValueType::Bytes => DynValueKind::Bytes,
            crate::ValueType::Array => DynValueKind::Array,
            crate::ValueType::Object => DynValueKind::Object,
        }
    }
}

unsafe fn dyn_get_bool(value: PtrConst<'_>) -> Option<bool> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_bool()
    }
}

unsafe fn dyn_get_i64(value: PtrConst<'_>) -> Option<i64> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_number().and_then(|n| n.to_i64())
    }
}

unsafe fn dyn_get_u64(value: PtrConst<'_>) -> Option<u64> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_number().and_then(|n| n.to_u64())
    }
}

unsafe fn dyn_get_f64(value: PtrConst<'_>) -> Option<f64> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_number().map(|n| n.to_f64_lossy())
    }
}

unsafe fn dyn_get_str<'a>(value: PtrConst<'a>) -> Option<&'a str> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_string().map(|s| s.as_str())
    }
}

unsafe fn dyn_get_bytes<'a>(value: PtrConst<'a>) -> Option<&'a [u8]> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_bytes().map(|b| b.as_slice())
    }
}

unsafe fn dyn_array_len(value: PtrConst<'_>) -> Option<usize> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_array().map(|a| a.len())
    }
}

unsafe fn dyn_array_get(value: PtrConst<'_>, index: usize) -> Option<PtrConst<'_>> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_array().and_then(|a| {
            a.get(index)
                .map(|elem| PtrConst::new(NonNull::new_unchecked(elem as *const Value as *mut u8)))
        })
    }
}

unsafe fn dyn_object_len(value: PtrConst<'_>) -> Option<usize> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_object().map(|o| o.len())
    }
}

unsafe fn dyn_object_get_entry<'a>(
    value: PtrConst<'a>,
    index: usize,
) -> Option<(&'a str, PtrConst<'a>)> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_object().and_then(|o| {
            o.iter().nth(index).map(|(k, v)| {
                (
                    k.as_str(),
                    PtrConst::new(NonNull::new_unchecked(v as *const Value as *mut u8)),
                )
            })
        })
    }
}

unsafe fn dyn_object_get<'a>(value: PtrConst<'a>, key: &str) -> Option<PtrConst<'a>> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_object().and_then(|o| {
            o.get(key)
                .map(|v| PtrConst::new(NonNull::new_unchecked(v as *const Value as *mut u8)))
        })
    }
}

// ============================================================================
// VTable and Shape
// ============================================================================

static DYNAMIC_VALUE_VTABLE: DynamicValueVTable = DynamicValueVTable::builder()
    .set_null(dyn_set_null)
    .set_bool(dyn_set_bool)
    .set_i64(dyn_set_i64)
    .set_u64(dyn_set_u64)
    .set_f64(dyn_set_f64)
    .set_str(dyn_set_str)
    .set_bytes(dyn_set_bytes)
    .begin_array(dyn_begin_array)
    .push_array_element(dyn_push_array_element)
    .begin_object(dyn_begin_object)
    .insert_object_entry(dyn_insert_object_entry)
    .get_kind(dyn_get_kind)
    .get_bool(dyn_get_bool)
    .get_i64(dyn_get_i64)
    .get_u64(dyn_get_u64)
    .get_f64(dyn_get_f64)
    .get_str(dyn_get_str)
    .get_bytes(dyn_get_bytes)
    .array_len(dyn_array_len)
    .array_get(dyn_array_get)
    .object_len(dyn_object_len)
    .object_get_entry(dyn_object_get_entry)
    .object_get(dyn_object_get)
    .build();

static DYNAMIC_VALUE_DEF: DynamicValueDef = DynamicValueDef::builder()
    .vtable(&DYNAMIC_VALUE_VTABLE)
    .build();

// Value vtable functions for the standard Facet machinery

unsafe fn value_drop_in_place(value: PtrMut<'_>) -> PtrUninit<'_> {
    unsafe {
        let ptr = value.as_mut_byte_ptr() as *mut Value;
        core::ptr::drop_in_place(ptr);
        PtrUninit::new(NonNull::new_unchecked(ptr as *mut u8))
    }
}

unsafe fn value_clone_into<'src, 'dst>(src: PtrConst<'src>, dst: PtrUninit<'dst>) -> PtrMut<'dst> {
    unsafe {
        let src_ptr = src.as_byte_ptr() as *const Value;
        let dst_ptr = dst.as_mut_byte_ptr() as *mut Value;
        dst_ptr.write((*src_ptr).clone());
        PtrMut::new(NonNull::new_unchecked(dst_ptr as *mut u8))
    }
}

unsafe fn value_debug(value: PtrConst<'_>, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        core::fmt::Debug::fmt(&*ptr, f)
    }
}

unsafe fn value_default_in_place(dst: PtrUninit<'_>) -> PtrMut<'_> {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(Value::default());
        PtrMut::new(NonNull::new_unchecked(ptr as *mut u8))
    }
}

unsafe fn value_partial_eq(a: PtrConst<'_>, b: PtrConst<'_>) -> bool {
    unsafe {
        let a_ptr = a.as_byte_ptr() as *const Value;
        let b_ptr = b.as_byte_ptr() as *const Value;
        *a_ptr == *b_ptr
    }
}

/// Wrapper to allow hashing through a `&mut dyn Hasher`
struct HasherWrapper<'a>(&'a mut dyn core::hash::Hasher);

impl core::hash::Hasher for HasherWrapper<'_> {
    fn finish(&self) -> u64 {
        self.0.finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }
}

unsafe fn value_hash(value: PtrConst<'_>, hasher: &mut dyn core::hash::Hasher) {
    unsafe {
        use core::hash::Hash;
        let ptr = value.as_byte_ptr() as *const Value;
        let mut wrapper = HasherWrapper(hasher);
        (*ptr).hash(&mut wrapper);
    }
}

fn value_type_name(f: &mut core::fmt::Formatter<'_>, _opts: TypeNameOpts) -> core::fmt::Result {
    write!(f, "Value")
}

static VALUE_VTABLE: ValueVTable = ValueVTable {
    type_name: value_type_name,
    marker_traits: MarkerTraits::SEND.union(MarkerTraits::SYNC),
    drop_in_place: Some(value_drop_in_place),
    invariants: None,
    display: None,
    debug: Some(value_debug),
    default_in_place: Some(value_default_in_place),
    clone_into: Some(value_clone_into),
    partial_eq: Some(value_partial_eq),
    partial_ord: None,
    ord: None,
    hash: Some(value_hash),
    parse: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
};

/// The static shape for `Value`.
pub static VALUE_SHAPE: Shape = Shape {
    id: ConstTypeId::of::<Value>(),
    layout: ShapeLayout::Sized(core::alloc::Layout::new::<Value>()),
    vtable: VALUE_VTABLE,
    ty: Type::User(UserType::Opaque),
    def: Def::DynamicValue(DYNAMIC_VALUE_DEF),
    type_identifier: "Value",
    type_params: &[],
    doc: &[" A dynamic value that can hold null, bool, number, string, bytes, array, or object."],
    attributes: &[],
    type_tag: None,
    inner: None,
};

unsafe impl Facet<'_> for Value {
    const SHAPE: &'static Shape = &VALUE_SHAPE;
}
