use std::io;

use super::*;

// r[verify channeling.id.parity]
#[test]
fn channel_id_allocator_initiator_uses_odd_ids() {
    let alloc = ChannelIdAllocator::new(Role::Initiator);
    assert_eq!(alloc.next(), 1);
    assert_eq!(alloc.next(), 3);
    assert_eq!(alloc.next(), 5);
    assert_eq!(alloc.next(), 7);
}

// r[verify channeling.id.parity]
#[test]
fn channel_id_allocator_acceptor_uses_even_ids() {
    let alloc = ChannelIdAllocator::new(Role::Acceptor);
    assert_eq!(alloc.next(), 2);
    assert_eq!(alloc.next(), 4);
    assert_eq!(alloc.next(), 6);
    assert_eq!(alloc.next(), 8);
}

// r[verify channeling.holder-semantics]
#[tokio::test]
async fn tx_serializes_and_rx_deserializes() {
    // Create a channel pair using roam::channel
    let (tx, mut rx) = channel::<i32>();

    // Simulate what ConnectionHandle::call would do: take the receiver
    let mut taken_rx = rx.receiver.take().expect("receiver should be present");

    // Now tx can send and we can receive on the taken receiver
    tx.send(&100).await.unwrap();
    tx.send(&200).await.unwrap();

    // Receive raw bytes and deserialize
    let bytes1 = taken_rx.recv().await.unwrap();
    let val1: i32 = facet_postcard::from_slice(&bytes1).unwrap();
    assert_eq!(val1, 100);

    let bytes2 = taken_rx.recv().await.unwrap();
    let val2: i32 = facet_postcard::from_slice(&bytes2).unwrap();
    assert_eq!(val2, 200);
}

/// Create a test registry with a dummy task channel.
fn test_registry() -> ChannelRegistry {
    let (task_tx, _task_rx) = crate::runtime::channel(10);
    ChannelRegistry::new(task_tx)
}

// r[verify channeling.data-after-close]
#[tokio::test]
async fn data_after_close_is_rejected() {
    let mut registry = test_registry();
    let (tx, _rx) = crate::runtime::channel(10);
    registry.register_incoming(42, tx);

    // Close the channel
    registry.close(42);

    // Data after close should fail
    let result = registry.route_data(42, b"data".to_vec()).await;
    assert_eq!(result, Err(ChannelError::DataAfterClose));
}

// r[verify channeling.data]
// r[verify channeling.unknown]
#[tokio::test]
async fn channel_registry_routes_data_to_registered_channel() {
    let mut registry = test_registry();

    // Register a channel
    let (tx, mut rx) = crate::runtime::channel(10);
    registry.register_incoming(42, tx);

    // Data to registered channel should succeed
    assert!(registry.route_data(42, b"hello".to_vec()).await.is_ok());

    // Should receive the data
    assert_eq!(rx.recv().await, Some(b"hello".to_vec()));

    // Data to unregistered channel should fail
    assert!(registry.route_data(999, b"nope".to_vec()).await.is_err());
}

// r[verify channeling.close]
#[tokio::test]
async fn channel_registry_close_terminates_channel() {
    let mut registry = test_registry();
    let (tx, mut rx) = crate::runtime::channel(10);
    registry.register_incoming(42, tx);

    // Send some data
    registry.route_data(42, b"data1".to_vec()).await.unwrap();

    // Close the channel
    registry.close(42);

    // Should still receive buffered data
    assert_eq!(rx.recv().await, Some(b"data1".to_vec()));

    // Then channel closes (sender dropped)
    assert_eq!(rx.recv().await, None);

    // Channel no longer registered
    assert!(!registry.contains(42));
}

#[test]
fn tx_rx_shape_metadata() {
    use facet::Facet;

    let tx_shape = <Tx<i32> as Facet>::SHAPE;
    let rx_shape = <Rx<i32> as Facet>::SHAPE;

    // Verify decl_id is consistent across different generic instantiations
    assert_eq!(tx_shape.decl_id, Tx::<()>::SHAPE.decl_id);
    assert_eq!(tx_shape.decl_id, Tx::<String>::SHAPE.decl_id);
    assert_eq!(rx_shape.decl_id, Rx::<()>::SHAPE.decl_id);
    assert_eq!(rx_shape.decl_id, Rx::<String>::SHAPE.decl_id);

    // Verify type_params are populated
    assert_eq!(tx_shape.type_params.len(), 1);
    assert_eq!(rx_shape.type_params.len(), 1);
}

// ========================================================================
// Tunnel Tests
// ========================================================================

#[tokio::test]
async fn tunnel_pair_connects_bidirectionally() {
    let (local, remote) = tunnel_pair();

    // Send from local to remote
    local.tx.send(&b"hello".to_vec()).await.unwrap();

    // Receive on remote
    let mut remote_rx = remote.rx;
    let received = remote_rx.recv().await.unwrap().unwrap();
    assert_eq!(received, b"hello".to_vec());

    // Send from remote to local
    remote.tx.send(&b"world".to_vec()).await.unwrap();

    // Receive on local
    let mut local_rx = local.rx;
    let received = local_rx.recv().await.unwrap().unwrap();
    assert_eq!(received, b"world".to_vec());
}

#[tokio::test]
async fn pump_read_to_tx_sends_chunks() {
    use std::io::Cursor;

    let data = b"hello world this is a test message";
    let reader = Cursor::new(data.to_vec());
    let (tx, mut rx) = channel::<Vec<u8>>();

    // Pump with small chunk size to force multiple chunks
    let handle = tokio::spawn(async move { pump_read_to_tx(reader, tx, 10).await });

    // Collect all received chunks
    let mut received = Vec::new();
    while let Ok(Some(chunk)) = rx.recv().await {
        received.extend(chunk);
    }

    // Verify we got all the data
    assert_eq!(received, data.to_vec());

    // Pump should complete successfully
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn pump_rx_to_write_writes_chunks() {
    use std::io::Cursor;

    let (tx, rx) = channel::<Vec<u8>>();
    let writer = Cursor::new(Vec::new());

    // Spawn pump task
    let handle = tokio::spawn(async move {
        let mut writer = writer;
        pump_rx_to_write(rx, &mut writer).await?;
        Ok::<_, io::Error>(writer)
    });

    // Send some chunks
    tx.send(&b"hello ".to_vec()).await.unwrap();
    tx.send(&b"world".to_vec()).await.unwrap();
    drop(tx); // Close the channel

    // Wait for pump to complete and get the writer
    let writer = handle.await.unwrap().unwrap();
    assert_eq!(writer.into_inner(), b"hello world".to_vec());
}

#[tokio::test]
async fn tunnel_stream_bidirectional() {
    // Create a duplex pair (simulates a socket)
    let (client, server) = tokio::io::duplex(1024);

    // Create tunnel pair
    let (local, remote) = tunnel_pair();

    // Tunnel the client side
    let (client_read_handle, client_write_handle) =
        tunnel_stream(client, local, DEFAULT_TUNNEL_CHUNK_SIZE);

    // Use remote tunnel to send/receive
    tokio::spawn(async move {
        // Send data through the tunnel (will go to server side of duplex)
        remote.tx.send(&b"from tunnel".to_vec()).await.unwrap();
    });

    // Read from server side of duplex
    let mut server = server;
    let mut buf = vec![0u8; 1024];
    let n = tokio::io::AsyncReadExt::read(&mut server, &mut buf)
        .await
        .unwrap();
    assert!(n > 0);

    // Write to server side
    tokio::io::AsyncWriteExt::write_all(&mut server, b"to tunnel")
        .await
        .unwrap();
    drop(server); // Close to signal EOF

    // Wait for read task to complete
    client_read_handle.await.unwrap().unwrap();
    client_write_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn tunnel_handles_empty_data() {
    let (tx, mut rx) = channel::<Vec<u8>>();

    // Sending empty vec should work
    tx.send(&Vec::new()).await.unwrap();

    let received = rx.recv().await.unwrap().unwrap();
    assert!(received.is_empty());
}

#[tokio::test]
async fn tunnel_close_propagates() {
    let (local, remote) = tunnel_pair();

    // Drop the sender
    drop(local.tx);

    // Receiver should see channel closed
    let mut rx = remote.rx;
    let result = rx.recv().await;
    assert!(matches!(result, Ok(None)));
}

// ========================================================================
// Channel ID Collection Tests
// ========================================================================

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_simple_tx() {
    let tx: Tx<i32> = Tx::try_from(42u64).unwrap();
    let ids = collect_channel_ids(&tx);
    assert_eq!(ids, vec![42]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_simple_rx() {
    let rx: Rx<i32> = Rx::try_from(99u64).unwrap();
    let ids = collect_channel_ids(&rx);
    assert_eq!(ids, vec![99]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_tuple() {
    let rx: Rx<String> = Rx::try_from(10u64).unwrap();
    let tx: Tx<String> = Tx::try_from(20u64).unwrap();
    let args = (rx, tx);
    let ids = collect_channel_ids(&args);
    assert_eq!(ids, vec![10, 20]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_nested_in_struct() {
    #[derive(facet::Facet)]
    struct StreamArgs {
        input: Rx<i32>,
        output: Tx<i32>,
        count: u32,
    }

    let args = StreamArgs {
        input: Rx::try_from(100u64).unwrap(),
        output: Tx::try_from(200u64).unwrap(),
        count: 5,
    };
    let ids = collect_channel_ids(&args);
    assert_eq!(ids, vec![100, 200]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_option_some() {
    let tx: Tx<i32> = Tx::try_from(55u64).unwrap();
    let args: Option<Tx<i32>> = Some(tx);
    let ids = collect_channel_ids(&args);
    assert_eq!(ids, vec![55]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_option_none() {
    let args: Option<Tx<i32>> = None;
    let ids = collect_channel_ids(&args);
    assert!(ids.is_empty());
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_vec() {
    let tx1: Tx<i32> = Tx::try_from(1u64).unwrap();
    let tx2: Tx<i32> = Tx::try_from(2u64).unwrap();
    let tx3: Tx<i32> = Tx::try_from(3u64).unwrap();
    let args: Vec<Tx<i32>> = vec![tx1, tx2, tx3];
    let ids = collect_channel_ids(&args);
    assert!(ids.is_empty());
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_deeply_nested() {
    #[derive(facet::Facet)]
    struct Outer {
        inner: Inner,
    }

    #[derive(facet::Facet)]
    struct Inner {
        channel: Tx<u8>,
    }

    let args = Outer {
        inner: Inner {
            channel: Tx::try_from(777u64).unwrap(),
        },
    };
    let ids = collect_channel_ids(&args);
    assert_eq!(ids, vec![777]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_large_bytes_payload_is_empty() {
    let args = vec![0xABu8; 512 * 1024];
    let ids = collect_channel_ids(&args);
    assert!(ids.is_empty());
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_large_bytes_payload_with_channel() {
    #[derive(facet::Facet)]
    struct ResponseLike {
        payload: Vec<u8>,
        channel: Tx<u8>,
    }

    let args = ResponseLike {
        payload: vec![0xCDu8; 512 * 1024],
        channel: Tx::try_from(4242u64).unwrap(),
    };
    let ids = collect_channel_ids(&args);
    assert_eq!(ids, vec![4242]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_enum_all_active_fields() {
    #[derive(facet::Facet)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum Multi {
        Unit = 0,
        Pair(Vec<u8>, Tx<u8>) = 1,
        Struct { left: Rx<u8>, right: Tx<u8> } = 2,
    }

    let pair = Multi::Pair(vec![1, 2, 3], Tx::try_from(11u64).unwrap());
    let struct_variant = Multi::Struct {
        left: Rx::try_from(22u64).unwrap(),
        right: Tx::try_from(33u64).unwrap(),
    };

    assert_eq!(collect_channel_ids(&pair), vec![11]);
    assert_eq!(collect_channel_ids(&struct_variant), vec![22, 33]);
    assert!(collect_channel_ids(&Multi::Unit).is_empty());
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_array_tuple_and_map_coverage() {
    #[derive(facet::Facet)]
    struct Complex {
        tuple: (u32, Tx<u8>, [Rx<u8>; 2]),
        map: std::collections::BTreeMap<String, Vec<u8>>,
        bytes: [u8; 16],
    }

    let mut map = std::collections::BTreeMap::new();
    map.insert("k".to_string(), vec![9, 9, 9]);

    let value = Complex {
        tuple: (
            7,
            Tx::try_from(100u64).unwrap(),
            [Rx::try_from(101u64).unwrap(), Rx::try_from(102u64).unwrap()],
        ),
        map,
        bytes: [0u8; 16],
    };

    let ids = collect_channel_ids(&value);
    assert_eq!(ids, vec![100]);
}

// r[verify call.request.channels]
#[test]
fn collect_channel_ids_shared_shape_branches_not_order_sensitive() {
    #[derive(facet::Facet)]
    struct Leaf {
        channel: Option<Tx<u8>>,
        payload: Vec<u8>,
    }

    #[derive(facet::Facet)]
    struct Root {
        a: Leaf,
        b: Leaf,
    }

    let value = Root {
        a: Leaf {
            channel: Some(Tx::try_from(5u64).unwrap()),
            payload: vec![1; 64 * 1024],
        },
        b: Leaf {
            channel: Some(Tx::try_from(6u64).unwrap()),
            payload: vec![2; 64 * 1024],
        },
    };

    let ids = collect_channel_ids(&value);
    assert_eq!(ids, vec![5, 6]);
}
