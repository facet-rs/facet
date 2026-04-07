//! FFI endpoint export for the Rust subject runtime.

use vox_ffi::declare_link_endpoint;

declare_link_endpoint!(pub mod subject_rust_ffi_endpoint {
    export = subject_rust_v1_vtable;
});
