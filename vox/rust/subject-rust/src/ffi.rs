//! FFI endpoint export for the Rust subject runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use spec_proto::{TestbedClient, TestbedDispatcher};
use tokio::runtime::Builder;
use vox::acceptor_on;
use vox_ffi::{Endpoint, VOX_STATUS_OK, vox_link_vtable, vox_status_t};

use crate::TestbedService;

static BOOTSTRAPPED: AtomicBool = AtomicBool::new(false);

fn endpoint_vtable() -> &'static vox_link_vtable {
    &ENDPOINT_VTABLE
}

static ENDPOINT: Endpoint = Endpoint::new(endpoint_vtable);

unsafe extern "C" fn endpoint_send(buf: *const u8, len: usize) {
    unsafe { vox_ffi::__endpoint_send(&ENDPOINT, buf, len) };
}

unsafe extern "C" fn endpoint_free(buf: *const u8) {
    unsafe { vox_ffi::__endpoint_free(&ENDPOINT, buf) };
}

unsafe extern "C" fn endpoint_attach(peer: *const vox_link_vtable) -> vox_status_t {
    let status = unsafe { vox_ffi::__endpoint_attach(&ENDPOINT, peer) };
    if status == VOX_STATUS_OK {
        bootstrap_service_once(peer);
    }
    status
}

static ENDPOINT_VTABLE: vox_link_vtable =
    vox_link_vtable::new(endpoint_send, endpoint_free, endpoint_attach);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn subject_rust_v1_vtable() -> *const vox_link_vtable {
    endpoint_vtable() as *const vox_link_vtable
}

fn bootstrap_service_once(peer: *const vox_link_vtable) {
    if peer.is_null() {
        return;
    }

    if BOOTSTRAPPED.swap(true, Ordering::AcqRel) {
        return;
    }

    let peer = peer as usize;
    thread::spawn(move || {
        let runtime = Builder::new_current_thread().enable_all().build();
        let Ok(runtime) = runtime else {
            return;
        };

        runtime.block_on(async move {
            let peer = peer as *const vox_link_vtable;
            let Some(peer) = (unsafe { peer.as_ref() }) else {
                return;
            };

            let Ok(link) = ENDPOINT.connect(peer) else {
                return;
            };

            let _client = acceptor_on(link)
                .on_connection(TestbedDispatcher::new(TestbedService))
                .establish::<TestbedClient>()
                .await
                .ok();

            std::future::pending::<()>().await;
        });
    });
}
