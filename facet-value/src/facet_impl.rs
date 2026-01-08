//! Facet implementation for Value, enabling deserialization from any format.

use facet_core::{
    Def, DynDateTimeKind, DynValueKind, DynamicValueDef, DynamicValueVTable, Facet, OxPtrConst,
    OxPtrMut, PtrConst, PtrMut, PtrUninit, Shape, ShapeBuilder, VTableErased,
};

use crate::{DateTimeKind, VArray, VBytes, VDateTime, VNumber, VObject, VString, Value};

// ============================================================================
// Scalar setters
// ============================================================================

unsafe fn dyn_set_null(dst: PtrUninit) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(Value::NULL);
    }
}

unsafe fn dyn_set_bool(dst: PtrUninit, value: bool) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(Value::from(value));
    }
}

unsafe fn dyn_set_i64(dst: PtrUninit, value: i64) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VNumber::from_i64(value).into_value());
    }
}

unsafe fn dyn_set_u64(dst: PtrUninit, value: u64) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VNumber::from_u64(value).into_value());
    }
}

unsafe fn dyn_set_f64(dst: PtrUninit, value: f64) -> bool {
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

unsafe fn dyn_set_str(dst: PtrUninit, value: &str) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VString::new(value).into_value());
    }
}

unsafe fn dyn_set_bytes(dst: PtrUninit, value: &[u8]) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VBytes::new(value).into_value());
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn dyn_set_datetime(
    dst: PtrUninit,
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nanos: u32,
    kind: DynDateTimeKind,
) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        let dt = match kind {
            DynDateTimeKind::Offset { offset_minutes } => VDateTime::new_offset(
                year,
                month,
                day,
                hour,
                minute,
                second,
                nanos,
                offset_minutes,
            ),
            DynDateTimeKind::LocalDateTime => {
                VDateTime::new_local_datetime(year, month, day, hour, minute, second, nanos)
            }
            DynDateTimeKind::LocalDate => VDateTime::new_local_date(year, month, day),
            DynDateTimeKind::LocalTime => VDateTime::new_local_time(hour, minute, second, nanos),
        };
        ptr.write(dt.into());
    }
}

// ============================================================================
// Array operations
// ============================================================================

unsafe fn dyn_begin_array(dst: PtrUninit) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VArray::new().into_value());
    }
}

unsafe fn dyn_push_array_element(array: PtrMut, element: PtrMut) {
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

unsafe fn dyn_begin_object(dst: PtrUninit) {
    unsafe {
        let ptr = dst.as_mut_byte_ptr() as *mut Value;
        ptr.write(VObject::new().into_value());
    }
}

unsafe fn dyn_insert_object_entry(object: PtrMut, key: &str, value: PtrMut) {
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

unsafe fn dyn_get_kind(value: PtrConst) -> DynValueKind {
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
            crate::ValueType::DateTime => DynValueKind::DateTime,
            crate::ValueType::QName => DynValueKind::QName,
            crate::ValueType::Uuid => DynValueKind::Uuid,
        }
    }
}

unsafe fn dyn_get_bool(value: PtrConst) -> Option<bool> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_bool()
    }
}

unsafe fn dyn_get_i64(value: PtrConst) -> Option<i64> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_number().and_then(|n| n.to_i64())
    }
}

unsafe fn dyn_get_u64(value: PtrConst) -> Option<u64> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_number().and_then(|n| n.to_u64())
    }
}

unsafe fn dyn_get_f64(value: PtrConst) -> Option<f64> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_number().map(|n| n.to_f64_lossy())
    }
}

unsafe fn dyn_get_str<'a>(value: PtrConst) -> Option<&'a str> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_string().map(|s| s.as_str())
    }
}

unsafe fn dyn_get_bytes<'a>(value: PtrConst) -> Option<&'a [u8]> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_bytes().map(|b| b.as_slice())
    }
}

#[allow(clippy::type_complexity)]
unsafe fn dyn_get_datetime(
    value: PtrConst,
) -> Option<(i32, u8, u8, u8, u8, u8, u32, DynDateTimeKind)> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_datetime().map(|dt| {
            let kind = match dt.kind() {
                DateTimeKind::Offset { offset_minutes } => {
                    DynDateTimeKind::Offset { offset_minutes }
                }
                DateTimeKind::LocalDateTime => DynDateTimeKind::LocalDateTime,
                DateTimeKind::LocalDate => DynDateTimeKind::LocalDate,
                DateTimeKind::LocalTime => DynDateTimeKind::LocalTime,
            };
            (
                dt.year(),
                dt.month(),
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second(),
                dt.nanos(),
                kind,
            )
        })
    }
}

unsafe fn dyn_array_len(value: PtrConst) -> Option<usize> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_array().map(|a| a.len())
    }
}

unsafe fn dyn_array_get(value: PtrConst, index: usize) -> Option<PtrConst> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr)
            .as_array()
            .and_then(|a| a.get(index).map(|elem| PtrConst::new(elem as *const Value)))
    }
}

unsafe fn dyn_object_len(value: PtrConst) -> Option<usize> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_object().map(|o| o.len())
    }
}

unsafe fn dyn_object_get_entry<'a>(value: PtrConst, index: usize) -> Option<(&'a str, PtrConst)> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr).as_object().and_then(|o| {
            o.iter()
                .nth(index)
                .map(|(k, v)| (k.as_str(), PtrConst::new(v as *const Value)))
        })
    }
}

unsafe fn dyn_object_get(value: PtrConst, key: &str) -> Option<PtrConst> {
    unsafe {
        let ptr = value.as_byte_ptr() as *const Value;
        (*ptr)
            .as_object()
            .and_then(|o| o.get(key).map(|v| PtrConst::new(v as *const Value)))
    }
}

unsafe fn dyn_object_get_mut(value: PtrMut, key: &str) -> Option<PtrMut> {
    unsafe {
        let ptr = value.as_mut_byte_ptr() as *mut Value;
        (*ptr)
            .as_object_mut()
            .and_then(|o| o.get_mut(key).map(|v| PtrMut::new(v as *mut Value)))
    }
}

// ============================================================================
// VTable and Shape
// ============================================================================

static DYNAMIC_VALUE_VTABLE: DynamicValueVTable = DynamicValueVTable {
    set_null: dyn_set_null,
    set_bool: dyn_set_bool,
    set_i64: dyn_set_i64,
    set_u64: dyn_set_u64,
    set_f64: dyn_set_f64,
    set_str: dyn_set_str,
    set_bytes: Some(dyn_set_bytes),
    set_datetime: Some(dyn_set_datetime),
    begin_array: dyn_begin_array,
    push_array_element: dyn_push_array_element,
    end_array: None,
    begin_object: dyn_begin_object,
    insert_object_entry: dyn_insert_object_entry,
    end_object: None,
    get_kind: dyn_get_kind,
    get_bool: dyn_get_bool,
    get_i64: dyn_get_i64,
    get_u64: dyn_get_u64,
    get_f64: dyn_get_f64,
    get_str: dyn_get_str,
    get_bytes: Some(dyn_get_bytes),
    get_datetime: Some(dyn_get_datetime),
    array_len: dyn_array_len,
    array_get: dyn_array_get,
    object_len: dyn_object_len,
    object_get_entry: dyn_object_get_entry,
    object_get: dyn_object_get,
    object_get_mut: Some(dyn_object_get_mut),
};

static DYNAMIC_VALUE_DEF: DynamicValueDef = DynamicValueDef::new(&DYNAMIC_VALUE_VTABLE);

// Value vtable functions for the standard Facet machinery

unsafe fn value_drop_in_place(ox: OxPtrMut) {
    unsafe {
        let ptr = ox.ptr().as_mut_byte_ptr() as *mut Value;
        core::ptr::drop_in_place(ptr);
    }
}

unsafe fn value_clone_into(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let src_ptr = src.ptr().as_byte_ptr() as *const Value;
        let dst_ptr = dst.ptr().as_mut_byte_ptr() as *mut Value;
        dst_ptr.write((*src_ptr).clone());
    }
}

unsafe fn value_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let ptr = ox.ptr().as_byte_ptr() as *const Value;
        Some(core::fmt::Debug::fmt(&*ptr, f))
    }
}

unsafe fn value_default_in_place(ox: OxPtrMut) {
    unsafe {
        let ptr = ox.ptr().as_mut_byte_ptr() as *mut Value;
        ptr.write(Value::default());
    }
}

unsafe fn value_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a_ptr = a.ptr().as_byte_ptr() as *const Value;
        let b_ptr = b.ptr().as_byte_ptr() as *const Value;
        Some(*a_ptr == *b_ptr)
    }
}

unsafe fn value_hash(ox: OxPtrConst, hasher: &mut facet_core::HashProxy<'_>) -> Option<()> {
    unsafe {
        use core::hash::Hash;
        let ptr = ox.ptr().as_byte_ptr() as *const Value;
        (*ptr).hash(hasher);
        Some(())
    }
}

// Use VTableIndirect for Value (trait operations)
static VALUE_VTABLE_INDIRECT: facet_core::VTableIndirect = facet_core::VTableIndirect {
    debug: Some(value_debug),
    partial_eq: Some(value_partial_eq),
    hash: Some(value_hash),
    ..facet_core::VTableIndirect::EMPTY
};

// Use TypeOpsIndirect for Value (type operations)
static VALUE_TYPE_OPS_INDIRECT: facet_core::TypeOpsIndirect = facet_core::TypeOpsIndirect {
    drop_in_place: value_drop_in_place,
    default_in_place: Some(value_default_in_place),
    clone_into: Some(value_clone_into),
    is_truthy: None,
};

unsafe impl Facet<'_> for Value {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Value>("Value")
            .vtable(VTableErased::Indirect(&VALUE_VTABLE_INDIRECT))
            .type_ops(facet_core::TypeOps::Indirect(&VALUE_TYPE_OPS_INDIRECT))
            .def(Def::DynamicValue(DYNAMIC_VALUE_DEF))
            .doc(&[" A dynamic value that can hold null, bool, number, string, bytes, array, or object."])
            .build()
    };
}

/// The static shape for `Value`.
pub static VALUE_SHAPE: &Shape = <Value as Facet>::SHAPE;
