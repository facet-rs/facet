pub mod types;
pub mod layout;
pub mod ring;
pub mod doorbell;
pub mod shm;
pub mod alloc;
pub mod flow;
pub mod header;
pub mod frame;
pub mod error;
pub mod codec;
pub mod control;
pub mod channel;
pub mod session;
pub mod fault;
pub mod observe;
pub mod dispatch;
pub mod registry;
pub mod callback;
pub mod service_macro;

// Note: The define_service! macro is automatically exported at the crate root
// due to #[macro_export], so no re-export is needed

#[cfg(test)]
mod proptests;
