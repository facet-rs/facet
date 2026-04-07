//! Native FFI entrypoints for Swift↔Rust session interoperability tests.

use std::ffi::c_void;
use std::thread;

use spec_proto::{TestbedClient, TestbedDispatcher, TestbedService};
use tokio::{runtime::Builder, sync::oneshot};
use vox::{TransportMode, acceptor_on, initiator_on};
use vox_ffi::{self, declare_link_endpoint, vox_link_vtable};

declare_link_endpoint!(mod subject_rust_ffi_endpoint {
    export = subject_rust_v1_vtable;
});

struct AcceptState {
    stop_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

#[no_mangle]
pub unsafe extern "C" fn subject_rust_v1_start_acceptor() -> *mut c_void {
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let thread = thread::spawn(move || {
        let runtime = Builder::new_current_thread().enable_all().build();
        let Ok(runtime) = runtime else {
            return;
        };

        runtime.block_on(async move {
            tokio::select! {
                link = subject_rust_ffi_endpoint::accept() => {
                    let Ok(link) = link else { return; };
                    let _root = acceptor_on(link)
                        .on_connection(TestbedDispatcher::new(TestbedService))
                        .establish::<vox::NoopClient>()
                        .await
                        .expect("acceptor handshake failed");
                    let _ = stop_rx.await;
                }
                _ = stop_rx => {
                    // early stop before peer connects
                }
            }
        });
    });

    Box::into_raw(Box::new(AcceptState {
        stop_tx: Some(stop_tx),
        thread: Some(thread),
    })) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn subject_rust_v1_stop_acceptor(handle: *mut c_void) -> i32 {
    if handle.is_null() {
        return -1;
    }

    let mut state = unsafe { Box::from_raw(handle as *mut AcceptState) };

    if let Some(stop) = state.stop_tx.take() {
        let _ = stop.send(());
    }

    let Some(thread) = state.thread.take() else {
        return 0;
    };

    match thread.join() {
        Ok(()) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn subject_rust_v1_initiator_reverse(peer: *const vox_link_vtable) -> i32 {
    if peer.is_null() {
        return -1;
    }

    let Ok(runtime) = Builder::new_current_thread().enable_all().build() else {
        return -2;
    };

    let Ok(peer) = maybe_static_vtable(peer) else {
        return -3;
    };

    let run = async move {
        let link = subject_rust_ffi_endpoint::connect(peer)?;
        let client = initiator_on(link, TransportMode::Bare)
            .on_connection(TestbedDispatcher::new(TestbedService))
            .establish::<TestbedClient>()
            .await?;
        if client.reverse("rust-to-swift".into()).await != Ok("tixs-ot-tsur".to_owned()) {
            return Err(());
        }
        Ok::<(), ()>(())
    };

    match runtime.block_on(run) {
        Ok(()) => 0,
        Err(_) => -4,
    }
}

unsafe fn maybe_static_vtable(
    peer: *const vox_link_vtable,
) -> Result<&'static vox_link_vtable, ()> {
    if peer.is_null() {
        return Err(());
    }
    Ok(&*peer)
}
