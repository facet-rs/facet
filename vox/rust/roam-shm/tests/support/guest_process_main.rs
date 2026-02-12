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

use once_cell::sync::Lazy;
use roam_session::{ChannelRegistry, Context, RpcPlan, Rx, ServiceDispatcher, Tx, dispatch_call};
use roam_shm::driver::establish_guest;
use roam_shm::spawn::{SpawnArgs, die_with_parent};
use roam_shm::transport::ShmGuestTransport;
use std::pin::Pin;
use std::sync::Arc;

// ============================================================================
// RPC Plans
// ============================================================================

static STRING_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(RpcPlan::for_type::<String>);
static STRING_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<String>()));

static I32_I32_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(RpcPlan::for_type::<(i32, i32)>);
static I32_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<i32>()));

static RX_I32_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(RpcPlan::for_type::<Rx<i32>>);
static I64_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<i64>()));

static U32_TX_I32_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(RpcPlan::for_type::<(u32, Tx<i32>)>);
static UNIT_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<()>()));

/// Test service matching the one in driver.rs tests
#[derive(Clone)]
struct TestService;

impl ServiceDispatcher for TestService {
    fn method_ids(&self) -> Vec<u64> {
        vec![1, 2, 3, 4]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        match cx.method_id().raw() {
            // Echo method: returns the input unchanged
            1 => dispatch_call::<String, String, (), _, _>(
                &cx,
                payload,
                registry,
                &STRING_ARGS_PLAN,
                STRING_RESPONSE_PLAN.clone(),
                |input: String| async move { Ok(input) },
            ),
            // Add method: adds two numbers
            2 => dispatch_call::<(i32, i32), i32, (), _, _>(
                &cx,
                payload,
                registry,
                &I32_I32_ARGS_PLAN,
                I32_RESPONSE_PLAN.clone(),
                |(a, b): (i32, i32)| async move { Ok(a + b) },
            ),
            // Sum method: client streams numbers, server returns sum
            3 => dispatch_call::<Rx<i32>, i64, (), _, _>(
                &cx,
                payload,
                registry,
                &RX_I32_ARGS_PLAN,
                I64_RESPONSE_PLAN.clone(),
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
                &cx,
                payload,
                registry,
                &U32_TX_I32_ARGS_PLAN,
                UNIT_RESPONSE_PLAN.clone(),
                |(count, output): (u32, Tx<i32>)| async move {
                    for i in 0..count {
                        output.send(&(i as i32)).await.ok();
                    }
                    Ok(())
                },
            ),
            _ => roam_session::dispatch_unknown_method(&cx, registry),
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
        ShmGuestTransport::from_spawn_args(args).expect("failed to create guest transport");
    let (_handle, _incoming_connections, driver) = establish_guest(transport, TestService);

    // Run the driver until the host disconnects
    driver.run().await.expect("guest driver failed");
}
