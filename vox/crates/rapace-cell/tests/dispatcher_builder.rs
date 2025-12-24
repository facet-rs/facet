//! Test for DispatcherBuilder duplicate method_id detection and cell_service! macro.
//!
//! This validates the fix for issue #82, ensuring that:
//! 1. cell_service! macro uses the generated METHOD_IDS constant
//! 2. DispatcherBuilder detects duplicate method_id registrations
//! 3. Multi-service cells work correctly with O(1) HashMap dispatch

use rapace_cell::*;
use std::sync::Arc;

// Define two simple services for testing
#[allow(async_fn_in_trait)]
#[rapace::service]
trait ServiceA {
    async fn method_a(&self) -> String;
}

#[allow(async_fn_in_trait)]
#[rapace::service]
trait ServiceB {
    async fn method_b(&self) -> String;
}

struct ServiceAImpl;
struct ServiceBImpl;

impl ServiceA for ServiceAImpl {
    async fn method_a(&self) -> String {
        "Hello from Service A".to_string()
    }
}

impl ServiceB for ServiceBImpl {
    async fn method_b(&self) -> String {
        "Hello from Service B".to_string()
    }
}

#[test]
fn test_cell_service_macro_uses_method_ids() {
    // This test verifies that cell_service! can be used with just two parameters
    // and automatically uses the METHOD_IDS constant

    mod test_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceAServer<ServiceAImpl>, ServiceAImpl);

        pub fn get_method_ids() -> &'static [u32] {
            let service = CellService::from(ServiceAImpl);
            service.method_ids()
        }
    }

    let method_ids = test_cell::get_method_ids();

    // Should match the generated METHOD_IDS constant
    assert_eq!(method_ids, ServiceAServer::<ServiceAImpl>::METHOD_IDS);
    assert_eq!(method_ids.len(), 1); // ServiceA has one method
}

#[test]
#[should_panic(expected = "duplicate registration for method")]
fn test_duplicate_method_id_detection() {
    // This test verifies that DispatcherBuilder panics when the same method_id
    // is registered twice

    mod test_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceAServer<ServiceAImpl>, ServiceAImpl);

        pub fn build_dispatcher_twice() {
            // This should panic because we're adding the same service twice
            let _ = DispatcherBuilder::new()
                .add_service(CellService::from(ServiceAImpl))
                .add_service(CellService::from(ServiceAImpl)); // PANIC: duplicate method_id
        }
    }

    test_cell::build_dispatcher_twice();
}

#[test]
fn test_multi_service_dispatcher() {
    // This test verifies that multiple different services can be added
    // to the same dispatcher without conflicts

    mod service_a_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceAServer<ServiceAImpl>, ServiceAImpl);

        pub fn build_dispatcher(builder: DispatcherBuilder) -> DispatcherBuilder {
            builder.add_service(CellService::from(ServiceAImpl))
        }
    }

    mod service_b_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceBServer<ServiceBImpl>, ServiceBImpl);

        pub fn build_dispatcher(builder: DispatcherBuilder) -> DispatcherBuilder {
            builder.add_service(CellService::from(ServiceBImpl))
        }
    }

    // Verify that METHOD_IDS are different
    assert_ne!(
        ServiceAServer::<ServiceAImpl>::METHOD_IDS[0],
        ServiceBServer::<ServiceBImpl>::METHOD_IDS[0],
        "Services should have different method IDs"
    );

    // Should successfully build a multi-service dispatcher
    let builder = DispatcherBuilder::new();
    let builder = service_a_cell::build_dispatcher(builder);
    let builder = service_b_cell::build_dispatcher(builder);

    // If we got here without panicking, duplicate detection works correctly
    let buffer_pool = BufferPool::new();
    let _dispatcher = builder.build(buffer_pool);
}

#[tokio_test_lite::test]
async fn test_multi_service_dispatch_e2e() -> Result<(), Box<dyn std::error::Error>> {
    // End-to-end test: create a cell with multiple services and verify routing works

    mod service_a_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceAServer<ServiceAImpl>, ServiceAImpl);

        pub fn build_dispatcher(builder: DispatcherBuilder) -> DispatcherBuilder {
            builder.add_service(CellService::from(ServiceAImpl))
        }
    }

    mod service_b_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceBServer<ServiceBImpl>, ServiceBImpl);

        pub fn build_dispatcher(builder: DispatcherBuilder) -> DispatcherBuilder {
            builder.add_service(CellService::from(ServiceBImpl))
        }
    }

    // Start a multi-service cell
    let (client_transport, server_transport) = rapace::Transport::mem_pair();

    // Build the multi-service dispatcher
    let buffer_pool = BufferPool::new();
    let builder = DispatcherBuilder::new();
    let builder = service_a_cell::build_dispatcher(builder);
    let builder = service_b_cell::build_dispatcher(builder);
    let dispatcher = builder.build(buffer_pool);

    // Run the dispatcher in the background
    let dispatcher_handle = tokio::spawn(async move {
        loop {
            match server_transport.recv_frame().await {
                Ok(request) => {
                    let response = dispatcher(request).await;
                    match response {
                        Ok(frame) => {
                            if let Err(e) = server_transport.send_frame(frame).await {
                                tracing::error!(?e, "Failed to send response");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!(?e, "Dispatch error");
                            break;
                        }
                    }
                }
                Err(rapace::TransportError::Closed) => break,
                Err(e) => {
                    tracing::error!(?e, "Transport error");
                    break;
                }
            }
        }
    });

    // Create client session and spawn demux loop
    let session = Arc::new(RpcSession::new(client_transport.clone()));
    let session_clone = session.clone();
    tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Test ServiceA
    let client_a = ServiceAClient::new(session.clone());
    let result = client_a.method_a().await?;
    assert_eq!(result, "Hello from Service A");

    // Test ServiceB
    let client_b = ServiceBClient::new(session.clone());
    let result = client_b.method_b().await?;
    assert_eq!(result, "Hello from Service B");

    // Cleanup
    client_transport.close();
    dispatcher_handle.abort();

    Ok(())
}

#[test]
fn test_method_ids_match_constants() {
    // Verify that the cell_service! macro correctly uses the METHOD_IDS constant
    mod test_cell {
        use super::*;
        rapace_cell::cell_service!(ServiceAServer<ServiceAImpl>, ServiceAImpl);

        pub fn get_method_ids() -> &'static [u32] {
            let service = CellService::from(ServiceAImpl);
            service.method_ids()
        }
    }

    let method_ids = test_cell::get_method_ids();

    // The method_ids() should return exactly what METHOD_IDS contains
    assert_eq!(method_ids, ServiceAServer::<ServiceAImpl>::METHOD_IDS);

    // And it should contain the individual method ID constant
    assert!(method_ids.contains(&SERVICE_A_METHOD_ID_METHOD_A));
}
