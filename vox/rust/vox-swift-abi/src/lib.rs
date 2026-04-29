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
use std::panic::{AssertUnwindSafe, catch_unwind};

use vox_types::{SchemaPayload, TypeRef};

pub type vox_swift_status_t = i32;

pub const VOX_SWIFT_STATUS_OK: vox_swift_status_t = 0;
pub const VOX_SWIFT_STATUS_BAD_ABI: vox_swift_status_t = -1;
pub const VOX_SWIFT_STATUS_UNSUPPORTED: vox_swift_status_t = -2;
pub const VOX_SWIFT_STATUS_PANIC: vox_swift_status_t = -3;

pub const VOX_SWIFT_TYPE_DESCRIPTOR_MAGIC: u64 = 0x564f_5853_5746_5431;
pub const VOX_SWIFT_TYPE_DESCRIPTOR_ABI_VERSION: u32 = 1;
pub const VOX_SWIFT_CODEC_CONFIG_ABI_VERSION: u32 = 1;

pub type vox_swift_codec_direction_t = u32;

pub const VOX_SWIFT_CODEC_DIRECTION_ARGS: vox_swift_codec_direction_t = 0;
pub const VOX_SWIFT_CODEC_DIRECTION_RESPONSE: vox_swift_codec_direction_t = 1;

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

/// Owned byte buffer returned by Rust and released by `vox_swift_owned_bytes_free_v1`.
#[repr(C)]
#[derive(Debug)]
pub struct vox_swift_owned_bytes {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl vox_swift_owned_bytes {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
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

/// Input for compiling a process-local Swift codec.
///
/// The local root describes the concrete Swift value layout for this process.
/// The remote schema CBOR is the peer's postcard schema payload for the same
/// method direction.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_swift_codec_config {
    pub abi_version: u32,
    pub size: u32,
    pub method_id: u64,
    pub direction: vox_swift_codec_direction_t,
    pub local_root: *const vox_swift_type_descriptor,
    pub remote_schema_cbor: vox_swift_bytes,
}

impl vox_swift_codec_config {
    pub const fn new(
        method_id: u64,
        direction: vox_swift_codec_direction_t,
        local_root: *const vox_swift_type_descriptor,
        remote_schema_cbor: vox_swift_bytes,
    ) -> Self {
        Self {
            abi_version: VOX_SWIFT_CODEC_CONFIG_ABI_VERSION,
            size: size_of::<Self>() as u32,
            method_id,
            direction,
            local_root,
            remote_schema_cbor,
        }
    }
}

/// Process-local Swift codec handle returned by `vox_swift_codec_prepare_v1`.
///
/// Callers must treat this as opaque and release it with
/// `vox_swift_codec_release_v1`.
#[repr(C)]
pub struct vox_swift_codec {
    _private: [u8; 0],
}

#[allow(dead_code)]
struct SwiftCodec {
    method_id: u64,
    direction: vox_swift_codec_direction_t,
    local_root: *const vox_swift_type_descriptor,
    remote_root_schema_id: u64,
    remote_schema_count: usize,
    remote_schema_cbor: Vec<u8>,
}

struct PreparedCodecConfig {
    method_id: u64,
    direction: vox_swift_codec_direction_t,
    local_root: *const vox_swift_type_descriptor,
    remote_root_schema_id: u64,
    remote_schema_count: usize,
    remote_schema_cbor: Vec<u8>,
}

impl PreparedCodecConfig {
    /// # Safety
    ///
    /// `config` must point to a live `vox_swift_codec_config`.
    unsafe fn from_ptr(config: *const vox_swift_codec_config) -> io::Result<Self> {
        let config = unsafe { config.as_ref() }.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "swift codec config was null")
        })?;

        if config.abi_version != VOX_SWIFT_CODEC_CONFIG_ABI_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift codec config abi version mismatch",
            ));
        }
        if config.size != size_of::<vox_swift_codec_config>() as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift codec config size mismatch",
            ));
        }
        if config.direction != VOX_SWIFT_CODEC_DIRECTION_ARGS
            && config.direction != VOX_SWIFT_CODEC_DIRECTION_RESPONSE
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "swift codec config direction was invalid",
            ));
        }

        let local_root = unsafe { vox_swift_type_descriptor::validate_ptr(config.local_root)? };
        config.remote_schema_cbor.validate("remote_schema_cbor")?;

        let remote_schema_cbor = if config.remote_schema_cbor.len == 0 {
            Vec::new()
        } else {
            unsafe {
                std::slice::from_raw_parts(
                    config.remote_schema_cbor.ptr,
                    config.remote_schema_cbor.len,
                )
            }
            .to_vec()
        };

        let remote_payload = SchemaPayload::from_cbor(&remote_schema_cbor).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("remote schema payload CBOR was invalid: {error}"),
            )
        })?;

        let remote_root_schema_id = match remote_payload.root {
            TypeRef::Concrete { type_id, .. } => type_id.0,
            TypeRef::Var { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "remote schema payload root cannot be a type variable",
                ));
            }
        };

        Ok(Self {
            method_id: config.method_id,
            direction: config.direction,
            local_root,
            remote_root_schema_id,
            remote_schema_count: remote_payload.schemas.len(),
            remote_schema_cbor,
        })
    }
}

/// # Safety
///
/// `codec` must point to a live `SwiftCodec` allocation returned as a
/// `vox_swift_codec` handle.
unsafe fn swift_codec_from_ptr<'a>(codec: *const vox_swift_codec) -> io::Result<&'a SwiftCodec> {
    unsafe { (codec.cast::<SwiftCodec>()).as_ref() }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "codec was null"))
}

fn ffi_status(run: impl FnOnce() -> io::Result<()>) -> vox_swift_status_t {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(())) => VOX_SWIFT_STATUS_OK,
        Ok(Err(_)) => VOX_SWIFT_STATUS_BAD_ABI,
        Err(_) => VOX_SWIFT_STATUS_PANIC,
    }
}

/// Prepare a Swift codec handle for one method direction.
///
/// This currently validates and retains the local Swift descriptor pointer plus
/// the peer schema payload. The actual planner/JIT backend is intentionally not
/// wired yet; encode/decode entrypoints return `VOX_SWIFT_STATUS_UNSUPPORTED`.
///
/// # Safety
///
/// `config` must point to a live `vox_swift_codec_config`; `out_codec` must
/// point to writable storage for one codec pointer. The local descriptor graph
/// referenced by `config` must outlive the returned codec handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_codec_prepare_v1(
    config: *const vox_swift_codec_config,
    out_codec: *mut *mut vox_swift_codec,
) -> vox_swift_status_t {
    ffi_status(|| {
        if out_codec.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "out_codec was null",
            ));
        }
        unsafe {
            *out_codec = std::ptr::null_mut();
        }

        let prepared = unsafe { PreparedCodecConfig::from_ptr(config)? };
        let codec = Box::new(SwiftCodec {
            method_id: prepared.method_id,
            direction: prepared.direction,
            local_root: prepared.local_root,
            remote_root_schema_id: prepared.remote_root_schema_id,
            remote_schema_count: prepared.remote_schema_count,
            remote_schema_cbor: prepared.remote_schema_cbor,
        });

        unsafe {
            *out_codec = Box::into_raw(codec).cast::<vox_swift_codec>();
        }
        Ok(())
    })
}

/// Release a Swift codec handle returned by `vox_swift_codec_prepare_v1`.
///
/// # Safety
///
/// `codec` must be null or a pointer returned by `vox_swift_codec_prepare_v1`
/// that has not already been released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_codec_release_v1(codec: *mut vox_swift_codec) {
    if !codec.is_null() {
        unsafe {
            drop(Box::from_raw(codec.cast::<SwiftCodec>()));
        }
    }
}

/// Encode a Swift value into postcard bytes.
///
/// # Safety
///
/// `codec` must be a live codec handle, `value` must point to a live Swift
/// value matching the codec's local root descriptor, and `out_bytes` must point
/// to writable storage for one owned byte buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_codec_encode_v1(
    codec: *const vox_swift_codec,
    value: *const u8,
    out_bytes: *mut vox_swift_owned_bytes,
) -> vox_swift_status_t {
    let status = ffi_status(|| {
        unsafe {
            swift_codec_from_ptr(codec)?;
        }
        if value.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "value was null",
            ));
        }
        if out_bytes.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "out_bytes was null",
            ));
        }
        unsafe {
            *out_bytes = vox_swift_owned_bytes::empty();
        }
        Ok(())
    });
    if status != VOX_SWIFT_STATUS_OK {
        return status;
    }
    VOX_SWIFT_STATUS_UNSUPPORTED
}

/// Decode postcard bytes into caller-owned Swift value storage.
///
/// # Safety
///
/// `codec` must be a live codec handle, `input` must point to readable postcard
/// bytes, and `dst` must point to writable storage large enough for the codec's
/// local root Swift value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_codec_decode_v1(
    codec: *const vox_swift_codec,
    input: vox_swift_bytes,
    dst: *mut u8,
) -> vox_swift_status_t {
    let status = ffi_status(|| {
        unsafe {
            swift_codec_from_ptr(codec)?;
        }
        input.validate("input")?;
        if dst.is_null() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "dst was null"));
        }
        Ok(())
    });
    if status != VOX_SWIFT_STATUS_OK {
        return status;
    }
    VOX_SWIFT_STATUS_UNSUPPORTED
}

/// Release bytes returned through `vox_swift_owned_bytes`.
///
/// # Safety
///
/// `bytes` must be null or point to a buffer previously returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_owned_bytes_free_v1(bytes: *mut vox_swift_owned_bytes) {
    let Some(bytes) = (unsafe { bytes.as_mut() }) else {
        return;
    };
    if !bytes.ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(bytes.ptr, bytes.len, bytes.capacity));
        }
    }
    *bytes = vox_swift_owned_bytes::empty();
}

#[cfg(test)]
mod tests {
    use super::*;
    use vox_types::{SchemaHash, SchemaPayload};

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

    fn schema_payload_bytes(root_id: u64) -> Vec<u8> {
        SchemaPayload {
            schemas: Vec::new(),
            root: TypeRef::concrete(SchemaHash(root_id)),
        }
        .to_cbor()
        .0
    }

    #[test]
    fn codec_prepare_rejects_null_config() {
        let mut codec = std::ptr::null_mut();

        let status = unsafe { vox_swift_codec_prepare_v1(std::ptr::null(), &mut codec) };

        assert_eq!(status, VOX_SWIFT_STATUS_BAD_ABI);
        assert!(codec.is_null());
    }

    #[test]
    fn codec_prepare_rejects_bad_config_size() {
        let local = vox_swift_type_descriptor::empty();
        let remote_schema_cbor = schema_payload_bytes(7);
        let mut config = vox_swift_codec_config::new(
            42,
            VOX_SWIFT_CODEC_DIRECTION_ARGS,
            &local,
            vox_swift_bytes {
                ptr: remote_schema_cbor.as_ptr(),
                len: remote_schema_cbor.len(),
            },
        );
        config.size += 1;
        let mut codec = std::ptr::null_mut();

        let status = unsafe { vox_swift_codec_prepare_v1(&config, &mut codec) };

        assert_eq!(status, VOX_SWIFT_STATUS_BAD_ABI);
        assert!(codec.is_null());
    }

    #[test]
    fn codec_prepare_accepts_valid_descriptor_and_schema_payload() {
        let local = vox_swift_type_descriptor::empty();
        let remote_schema_cbor = schema_payload_bytes(7);
        let config = vox_swift_codec_config::new(
            42,
            VOX_SWIFT_CODEC_DIRECTION_RESPONSE,
            &local,
            vox_swift_bytes {
                ptr: remote_schema_cbor.as_ptr(),
                len: remote_schema_cbor.len(),
            },
        );
        let mut codec = std::ptr::null_mut();

        let status = unsafe { vox_swift_codec_prepare_v1(&config, &mut codec) };

        assert_eq!(status, VOX_SWIFT_STATUS_OK);
        assert!(!codec.is_null());

        let codec_ref = unsafe { (codec.cast::<SwiftCodec>()).as_ref().unwrap() };
        assert_eq!(codec_ref.method_id, 42);
        assert_eq!(codec_ref.direction, VOX_SWIFT_CODEC_DIRECTION_RESPONSE);
        assert!(std::ptr::eq(codec_ref.local_root, &local));
        assert_eq!(codec_ref.remote_root_schema_id, 7);
        assert_eq!(codec_ref.remote_schema_count, 0);
        assert_eq!(codec_ref.remote_schema_cbor, remote_schema_cbor);

        unsafe {
            vox_swift_codec_release_v1(codec);
        }
    }

    #[test]
    fn codec_encode_and_decode_are_explicitly_unsupported_until_jit_lands() {
        let local = vox_swift_type_descriptor::empty();
        let remote_schema_cbor = schema_payload_bytes(7);
        let config = vox_swift_codec_config::new(
            42,
            VOX_SWIFT_CODEC_DIRECTION_ARGS,
            &local,
            vox_swift_bytes {
                ptr: remote_schema_cbor.as_ptr(),
                len: remote_schema_cbor.len(),
            },
        );
        let mut codec = std::ptr::null_mut();
        assert_eq!(
            unsafe { vox_swift_codec_prepare_v1(&config, &mut codec) },
            VOX_SWIFT_STATUS_OK
        );

        let value = 5_u8;
        let mut out = vox_swift_owned_bytes::empty();
        assert_eq!(
            unsafe { vox_swift_codec_encode_v1(codec, &value, &mut out) },
            VOX_SWIFT_STATUS_UNSUPPORTED
        );

        let mut dst = 0_u8;
        assert_eq!(
            unsafe {
                vox_swift_codec_decode_v1(
                    codec,
                    vox_swift_bytes {
                        ptr: remote_schema_cbor.as_ptr(),
                        len: remote_schema_cbor.len(),
                    },
                    &mut dst,
                )
            },
            VOX_SWIFT_STATUS_UNSUPPORTED
        );

        unsafe {
            vox_swift_owned_bytes_free_v1(&mut out);
            vox_swift_codec_release_v1(codec);
        }
    }
}
