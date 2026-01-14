//! Test guest process that can be spawned for integration tests.
//!
//! This binary is spawned by integration tests to verify multi-process
//! communication over SHM. It:
//! - Parses spawn arguments from command line
//! - Attaches to the SHM segment using the provided doorbell fd
//! - Sets up a guest driver with a test service
//! - Handles RPC calls from the host
//!
//! Usage: spawned via SpawnTicket::spawn(), receives args automatically

use roam_session::{ChannelRegistry, Rx, ServiceDispatcher, Tx, dispatch_call};
use roam_shm::driver::establish_guest;
use roam_shm::spawn::{SpawnArgs, die_with_parent};
use roam_shm::transport::ShmGuestTransport;
use std::pin::Pin;

/// Test service matching the one in driver.rs tests
#[derive(Clone)]
struct TestService;

impl ServiceDispatcher for TestService {
    fn method_ids(&self) -> Vec<u64> {
        vec![1, 2, 3, 4]
    }

    fn dispatch(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        channels: Vec<u64>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        match method_id {
            // Echo method: returns the input unchanged
            1 => dispatch_call::<String, String, (), _, _>(
                payload,
                channels,
                request_id,
                registry,
                |input: String| async move { Ok(input) },
            ),
            // Add method: adds two numbers
            2 => dispatch_call::<(i32, i32), i32, (), _, _>(
                payload,
                channels,
                request_id,
                registry,
                |(a, b): (i32, i32)| async move { Ok(a + b) },
            ),
            // Sum method: client streams numbers, server returns sum
            3 => dispatch_call::<Rx<i32>, i64, (), _, _>(
                payload,
                channels,
                request_id,
                registry,
                |mut input: Rx<i32>| async move {
                    let mut sum: i64 = 0;
                    while let Ok(Some(value)) = input.recv().await {
                        sum += value as i64;
                    }
                    Ok(sum)
                },
            ),
            // Generate method: server streams numbers back to client
            4 => dispatch_call::<(u32, Tx<i32>), (), (), _, _>(
                payload,
                channels,
                request_id,
                registry,
                |(count, output): (u32, Tx<i32>)| async move {
                    for i in 0..count {
                        output.send(&(i as i32)).await.ok();
                    }
                    Ok(())
                },
            ),
            _ => roam_session::dispatch_unknown_method(request_id, registry),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Die if parent dies (even via SIGKILL)
    die_with_parent();

    // Parse spawn arguments from command line
    let args = SpawnArgs::from_env().expect("failed to parse spawn args");

    // Create guest transport from spawn args (includes doorbell setup)
    let transport =
        ShmGuestTransport::from_spawn_args(&args).expect("failed to create guest transport");
    let (_handle, driver) = establish_guest(transport, TestService);

    // Run the driver until the host disconnects
    driver.run().await.expect("guest driver failed");
}
