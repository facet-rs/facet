//! FFI endpoint export for the Rust subject runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use spec_proto::{TestbedClient, TestbedDispatcher};
use tokio::runtime::Builder;
use tracing::info;
use vox::acceptor_on;
use vox_ffi::{Endpoint, VOX_STATUS_OK, vox_link_vtable, vox_status_t};

use crate::TestbedService;

static BOOTSTRAPPED: AtomicBool = AtomicBool::new(false);
static TRACING_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn init_ffi_tracing_once() {
    if TRACING_INITIALIZED.swap(true, Ordering::AcqRel) {
        return;
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

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
    init_ffi_tracing_once();
    eprintln!("[subject-rust ffi] endpoint_attach(peer={peer:p})");
    let status = unsafe { vox_ffi::__endpoint_attach(&ENDPOINT, peer) };
    eprintln!("[subject-rust ffi] endpoint_attach -> status={status}");
    if status == VOX_STATUS_OK {
        bootstrap_service_once(peer);
    }
    status
}

static ENDPOINT_VTABLE: vox_link_vtable =
    vox_link_vtable::new(endpoint_send, endpoint_free, endpoint_attach);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn subject_rust_v1_vtable() -> *const vox_link_vtable {
    init_ffi_tracing_once();
    eprintln!("[subject-rust ffi] subject_rust_v1_vtable()");
    endpoint_vtable() as *const vox_link_vtable
}

fn bootstrap_service_once(peer: *const vox_link_vtable) {
    init_ffi_tracing_once();
    if peer.is_null() {
        eprintln!("[subject-rust ffi] bootstrap_service_once: null peer");
        return;
    }

    if BOOTSTRAPPED.swap(true, Ordering::AcqRel) {
        eprintln!("[subject-rust ffi] bootstrap_service_once: already bootstrapped");
        return;
    }

    eprintln!("[subject-rust ffi] bootstrap_service_once: spawning runtime thread");
    let peer = peer as usize;
    thread::spawn(move || {
        eprintln!("[subject-rust ffi] runtime thread: starting");
        let runtime = Builder::new_current_thread().enable_all().build();
        let Ok(runtime) = runtime else {
            eprintln!("[subject-rust ffi] runtime thread: failed to create tokio runtime");
            return;
        };

        runtime.block_on(async move {
            let peer = peer as *const vox_link_vtable;
            let Some(peer) = (unsafe { peer.as_ref() }) else {
                eprintln!("[subject-rust ffi] runtime thread: peer pointer became null");
                return;
            };
            info!("ffi runtime: peer attached, opening link");
            eprintln!("[subject-rust ffi] runtime thread: connecting endpoint to peer");

            let Ok(link) = ENDPOINT.connect(peer) else {
                eprintln!("[subject-rust ffi] runtime thread: ENDPOINT.connect failed");
                return;
            };
            eprintln!("[subject-rust ffi] runtime thread: ENDPOINT.connect succeeded");

            let establish = acceptor_on(link)
                .on_connection(TestbedDispatcher::new(TestbedService))
                .establish::<TestbedClient>()
                .await;
            match establish {
                Ok(_client) => {
                    info!("ffi runtime: acceptor established");
                    eprintln!("[subject-rust ffi] runtime thread: establish succeeded");
                }
                Err(error) => {
                    eprintln!("[subject-rust ffi] runtime thread: establish failed: {error}");
                    return;
                }
            }

            eprintln!("[subject-rust ffi] runtime thread: parked after establish");
            std::future::pending::<()>().await;
        });
    });
}
