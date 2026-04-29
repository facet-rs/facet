//! Process-local Swift value descriptor ABI for vox.
//!
//! This crate intentionally does not define a stable cross-process Swift ABI.
//! Descriptors are emitted by Swift code running in the same process as the
//! Rust codec/JIT and describe the concrete Swift layout of that process.

#![allow(non_camel_case_types)]
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::io;
use std::mem::size_of;

pub type vox_swift_status_t = i32;

pub const VOX_SWIFT_STATUS_OK: vox_swift_status_t = 0;
pub const VOX_SWIFT_STATUS_BAD_ABI: vox_swift_status_t = -1;
pub const VOX_SWIFT_STATUS_UNSUPPORTED: vox_swift_status_t = -2;
pub const VOX_SWIFT_STATUS_PANIC: vox_swift_status_t = -3;

pub const VOX_SWIFT_TYPE_DESCRIPTOR_MAGIC: u64 = 0x564f_5853_5746_5431;
pub const VOX_SWIFT_TYPE_DESCRIPTOR_ABI_VERSION: u32 = 1;

pub type vox_swift_type_kind_t = u32;

pub const VOX_SWIFT_TYPE_KIND_PRIMITIVE: vox_swift_type_kind_t = 0;
pub const VOX_SWIFT_TYPE_KIND_STRUCT: vox_swift_type_kind_t = 1;
pub const VOX_SWIFT_TYPE_KIND_ENUM: vox_swift_type_kind_t = 2;
pub const VOX_SWIFT_TYPE_KIND_TUPLE: vox_swift_type_kind_t = 3;
pub const VOX_SWIFT_TYPE_KIND_LIST: vox_swift_type_kind_t = 4;
pub const VOX_SWIFT_TYPE_KIND_MAP: vox_swift_type_kind_t = 5;
pub const VOX_SWIFT_TYPE_KIND_ARRAY: vox_swift_type_kind_t = 6;
pub const VOX_SWIFT_TYPE_KIND_OPTION: vox_swift_type_kind_t = 7;
pub const VOX_SWIFT_TYPE_KIND_STRING: vox_swift_type_kind_t = 8;
pub const VOX_SWIFT_TYPE_KIND_BYTES: vox_swift_type_kind_t = 9;
pub const VOX_SWIFT_TYPE_KIND_CHANNEL: vox_swift_type_kind_t = 10;
pub const VOX_SWIFT_TYPE_KIND_OPAQUE: vox_swift_type_kind_t = 11;

pub type vox_swift_primitive_kind_t = u32;

pub const VOX_SWIFT_PRIMITIVE_UNIT: vox_swift_primitive_kind_t = 0;
pub const VOX_SWIFT_PRIMITIVE_BOOL: vox_swift_primitive_kind_t = 1;
pub const VOX_SWIFT_PRIMITIVE_U8: vox_swift_primitive_kind_t = 2;
pub const VOX_SWIFT_PRIMITIVE_U16: vox_swift_primitive_kind_t = 3;
pub const VOX_SWIFT_PRIMITIVE_U32: vox_swift_primitive_kind_t = 4;
pub const VOX_SWIFT_PRIMITIVE_U64: vox_swift_primitive_kind_t = 5;
pub const VOX_SWIFT_PRIMITIVE_I8: vox_swift_primitive_kind_t = 6;
pub const VOX_SWIFT_PRIMITIVE_I16: vox_swift_primitive_kind_t = 7;
pub const VOX_SWIFT_PRIMITIVE_I32: vox_swift_primitive_kind_t = 8;
pub const VOX_SWIFT_PRIMITIVE_I64: vox_swift_primitive_kind_t = 9;
pub const VOX_SWIFT_PRIMITIVE_F32: vox_swift_primitive_kind_t = 10;
pub const VOX_SWIFT_PRIMITIVE_F64: vox_swift_primitive_kind_t = 11;

pub type vox_swift_type_flags_t = u32;

pub const VOX_SWIFT_TYPE_FLAG_TRIVIAL: vox_swift_type_flags_t = 1 << 0;
pub const VOX_SWIFT_TYPE_FLAG_BITWISE_MOVABLE: vox_swift_type_flags_t = 1 << 1;
pub const VOX_SWIFT_TYPE_FLAG_HAS_DEFAULT: vox_swift_type_flags_t = 1 << 2;
pub const VOX_SWIFT_TYPE_FLAG_FIXED_LAYOUT: vox_swift_type_flags_t = 1 << 3;

pub type vox_swift_field_flags_t = u32;

pub const VOX_SWIFT_FIELD_FLAG_HAS_DEFAULT: vox_swift_field_flags_t = 1 << 0;

/// Borrowed UTF-8 bytes with process-local lifetime.
///
/// Generated Swift should point this at static storage.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct vox_swift_bytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl vox_swift_bytes {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub fn validate(&self, field: &str) -> io::Result<()> {
        if self.len != 0 && self.ptr.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{field} has non-zero length with null pointer"),
            ));
        }
        Ok(())
    }
}

pub type vox_swift_destroy_fn = unsafe extern "C" fn(value: *mut u8, context: *const c_void);
pub type vox_swift_copy_init_fn = unsafe extern "C" fn(
    dst: *mut u8,
    src: *const u8,
    context: *const c_void,
) -> vox_swift_status_t;
pub type vox_swift_take_init_fn =
    unsafe extern "C" fn(dst: *mut u8, src: *mut u8, context: *const c_void) -> vox_swift_status_t;
pub type vox_swift_default_init_fn =
    unsafe extern "C" fn(dst: *mut u8, context: *const c_void) -> vox_swift_status_t;

/// Value lifetime operations for one concrete Swift type.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_swift_value_witnesses {
    pub destroy: Option<vox_swift_destroy_fn>,
    pub copy_init: Option<vox_swift_copy_init_fn>,
    pub take_init: Option<vox_swift_take_init_fn>,
    pub default_init: Option<vox_swift_default_init_fn>,
}

impl vox_swift_value_witnesses {
    pub const fn empty() -> Self {
        Self {
            destroy: None,
            copy_init: None,
            take_init: None,
            default_init: None,
        }
    }
}

pub type vox_swift_enum_field_visitor_fn = unsafe extern "C" fn(
    visitor_context: *mut c_void,
    field_index: usize,
    field_ptr: *const u8,
) -> vox_swift_status_t;
pub type vox_swift_enum_tag_fn =
    unsafe extern "C" fn(value: *const u8, context: *const c_void) -> u32;
pub type vox_swift_enum_project_fn = unsafe extern "C" fn(
    value: *const u8,
    variant_index: u32,
    visitor_context: *mut c_void,
    visitor: Option<vox_swift_enum_field_visitor_fn>,
    context: *const c_void,
) -> vox_swift_status_t;
pub type vox_swift_enum_inject_fn = unsafe extern "C" fn(
    dst: *mut u8,
    variant_index: u32,
    field_values: *const *const u8,
    field_count: usize,
    context: *const c_void,
) -> vox_swift_status_t;

/// Enum case operations for Swift enums whose layout should not be assumed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_swift_enum_witnesses {
    pub tag: Option<vox_swift_enum_tag_fn>,
    pub project: Option<vox_swift_enum_project_fn>,
    pub inject: Option<vox_swift_enum_inject_fn>,
}

impl vox_swift_enum_witnesses {
    pub const fn empty() -> Self {
        Self {
            tag: None,
            project: None,
            inject: None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_swift_field_descriptor {
    pub name: vox_swift_bytes,
    pub schema_id: u64,
    pub ty: *const vox_swift_type_descriptor,
    pub offset: usize,
    pub flags: vox_swift_field_flags_t,
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_swift_variant_descriptor {
    pub name: vox_swift_bytes,
    pub index: u32,
    pub _reserved: u32,
    pub fields: *const vox_swift_field_descriptor,
    pub field_count: usize,
}

/// Concrete Swift type descriptor consumed by Rust codec planning/JIT.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_swift_type_descriptor {
    pub magic: u64,
    pub abi_version: u32,
    pub size: u32,
    pub kind: vox_swift_type_kind_t,
    pub primitive_kind: vox_swift_primitive_kind_t,
    pub flags: vox_swift_type_flags_t,
    pub schema_id: u64,
    pub type_metadata: *const c_void,
    pub value_size: usize,
    pub value_stride: usize,
    pub value_align: usize,
    pub type_args: *const *const vox_swift_type_descriptor,
    pub type_arg_count: usize,
    pub fields: *const vox_swift_field_descriptor,
    pub field_count: usize,
    pub variants: *const vox_swift_variant_descriptor,
    pub variant_count: usize,
    pub witnesses: vox_swift_value_witnesses,
    pub enum_witnesses: vox_swift_enum_witnesses,
    pub context: *const c_void,
}

impl vox_swift_type_descriptor {
    pub const fn empty() -> Self {
        Self {
            magic: VOX_SWIFT_TYPE_DESCRIPTOR_MAGIC,
            abi_version: VOX_SWIFT_TYPE_DESCRIPTOR_ABI_VERSION,
            size: size_of::<Self>() as u32,
            kind: VOX_SWIFT_TYPE_KIND_OPAQUE,
            primitive_kind: VOX_SWIFT_PRIMITIVE_UNIT,
            flags: 0,
            schema_id: 0,
            type_metadata: std::ptr::null(),
            value_size: 0,
            value_stride: 0,
            value_align: 1,
            type_args: std::ptr::null(),
            type_arg_count: 0,
            fields: std::ptr::null(),
            field_count: 0,
            variants: std::ptr::null(),
            variant_count: 0,
            witnesses: vox_swift_value_witnesses::empty(),
            enum_witnesses: vox_swift_enum_witnesses::empty(),
            context: std::ptr::null(),
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.magic != VOX_SWIFT_TYPE_DESCRIPTOR_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor magic mismatch",
            ));
        }
        if self.abi_version != VOX_SWIFT_TYPE_DESCRIPTOR_ABI_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor abi version mismatch",
            ));
        }
        if self.size != size_of::<Self>() as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor size mismatch",
            ));
        }
        if self.value_align == 0 || !self.value_align.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor value_align must be a non-zero power of two",
            ));
        }
        if self.value_stride < self.value_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor stride smaller than size",
            ));
        }
        if self.type_arg_count != 0 && self.type_args.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor has type_arg_count with null type_args",
            ));
        }
        if self.field_count != 0 && self.fields.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor has field_count with null fields",
            ));
        }
        if self.variant_count != 0 && self.variants.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift type descriptor has variant_count with null variants",
            ));
        }
        if self.kind == VOX_SWIFT_TYPE_KIND_ENUM
            && (self.enum_witnesses.tag.is_none()
                || self.enum_witnesses.inject.is_none()
                || self.enum_witnesses.project.is_none())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift enum descriptor missing enum witnesses",
            ));
        }
        Ok(())
    }

    /// # Safety
    ///
    /// `ptr` must either be null or point to a live
    /// `vox_swift_type_descriptor`.
    pub unsafe fn validate_ptr<'a>(ptr: *const Self) -> io::Result<&'a Self> {
        let desc = unsafe { ptr.as_ref() }.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "swift type descriptor pointer was null",
            )
        })?;
        desc.validate()?;
        Ok(desc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_descriptor_is_valid_opaque_descriptor() {
        vox_swift_type_descriptor::empty().validate().unwrap();
    }

    #[test]
    fn rejects_bad_magic() {
        let mut desc = vox_swift_type_descriptor::empty();
        desc.magic ^= 1;
        assert!(matches!(
            desc.validate(),
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));
    }

    #[test]
    fn rejects_count_without_pointer() {
        let mut desc = vox_swift_type_descriptor::empty();
        desc.field_count = 1;
        assert!(matches!(
            desc.validate(),
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));
    }

    #[test]
    fn rejects_enum_without_witnesses() {
        let mut desc = vox_swift_type_descriptor::empty();
        desc.kind = VOX_SWIFT_TYPE_KIND_ENUM;
        assert!(matches!(
            desc.validate(),
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));
    }
}
