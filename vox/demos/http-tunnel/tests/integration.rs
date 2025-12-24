//! Integration tests for HTTP tunnel over rapace.
//!
//! These tests verify that real HTTP traffic can flow through rapace tunnels.

use std::sync::Arc;
use std::time::Duration;

use rapace::{RpcSession, Transport};

use rapace_http_tunnel::{
    GlobalTunnelMetrics, TcpTunnelImpl, TunnelHost, create_tunnel_dispatcher, run_http_server,
};

/// Helper to start the plugin side (HTTP server + tunnel service).
async fn start_plugin(session: Arc<RpcSession>, http_port: u16) -> Arc<GlobalTunnelMetrics> {
    let metrics = Arc::new(GlobalTunnelMetrics::new());

    // Create the tunnel service
    let tunnel_service = Arc::new(TcpTunnelImpl::with_metrics(
        session.clone(),
        http_port,
        metrics.clone(),
    ));

    // Set dispatcher
    session.set_dispatcher(create_tunnel_dispatcher(
        tunnel_service,
        session.buffer_pool().clone(),
    ));

    // Spawn demux loop
    let session_clone = session.clone();
    tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Start HTTP server
    tokio::spawn(async move {
        let _ = run_http_server(http_port).await;
    });

    // Give HTTP server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    metrics
}

/// Helper to start the host side (tunnel client).
async fn start_host(session: Arc<RpcSession>) -> Arc<TunnelHost> {
    // Spawn demux loop
    let session_clone = session.clone();
    tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    Arc::new(TunnelHost::new(session))
}

#[tokio_test_lite::test]
async fn test_hello_endpoint() {
    // Use a unique port for this test
    let http_port = 19876;

    // Create transport pair
    let (host_transport, plugin_transport) = Transport::mem_pair();

    // Start plugin (even channel IDs)
    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport, 2));
    let _plugin_metrics = start_plugin(plugin_session, http_port).await;

    // Start host (odd channel IDs)
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
    let host = start_host(host_session).await;

    // Start a mini TCP proxy server for the test
    let host_clone = host.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen_addr = listener.local_addr().unwrap();

    // Spawn accept loop
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let host = host_clone.clone();
                tokio::spawn(async move {
                    let _ = host.handle_connection(stream).await;
                });
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Make HTTP request via reqwest
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/hello", listen_addr))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "hello from tunnel");
}

#[tokio_test_lite::test]
async fn test_health_endpoint() {
    let http_port = 19877;

    let (host_transport, plugin_transport) = Transport::mem_pair();

    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport, 2));
    let _plugin_metrics = start_plugin(plugin_session, http_port).await;

    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
    let host = start_host(host_session).await;

    let host_clone = host.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let host = host_clone.clone();
                tokio::spawn(async move {
                    let _ = host.handle_connection(stream).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/health", listen_addr))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "ok");
}

#[tokio_test_lite::test]
async fn test_echo_endpoint() {
    let http_port = 19878;

    let (host_transport, plugin_transport) = Transport::mem_pair();

    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport, 2));
    let _plugin_metrics = start_plugin(plugin_session, http_port).await;

    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
    let host = start_host(host_session).await;

    let host_clone = host.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let host = host_clone.clone();
                tokio::spawn(async move {
                    let _ = host.handle_connection(stream).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let test_body = "Hello, rapace tunnel!";
    let resp = client
        .post(format!("http://{}/echo", listen_addr))
        .body(test_body)
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, test_body);
}

#[tokio_test_lite::test]
async fn test_multiple_requests() {
    let http_port = 19879;

    let (host_transport, plugin_transport) = Transport::mem_pair();

    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport, 2));
    let _plugin_metrics = start_plugin(plugin_session, http_port).await;

    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
    let host = start_host(host_session).await;

    let host_clone = host.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let host = host_clone.clone();
                tokio::spawn(async move {
                    let _ = host.handle_connection(stream).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Make multiple requests in sequence
    let client = reqwest::Client::new();

    for i in 0..5 {
        let resp = client
            .get(format!("http://{}/hello", listen_addr))
            .send()
            .await
            .expect("request should succeed");

        assert_eq!(resp.status(), 200, "request {} failed", i);
        let body = resp.text().await.unwrap();
        assert_eq!(body, "hello from tunnel");
    }
}

#[tokio_test_lite::test]
async fn test_concurrent_requests() {
    let http_port = 19880;

    let (host_transport, plugin_transport) = Transport::mem_pair();

    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport, 2));
    let _plugin_metrics = start_plugin(plugin_session, http_port).await;

    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
    let host = start_host(host_session).await;

    let host_clone = host.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let host = host_clone.clone();
                tokio::spawn(async move {
                    let _ = host.handle_connection(stream).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Make concurrent requests
    let mut handles = vec![];
    for _ in 0..10 {
        let addr = listen_addr;
        handles.push(tokio::spawn(async move {
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("http://{}/hello", addr))
                .send()
                .await
                .expect("request should succeed");

            assert_eq!(resp.status(), 200);
            let body = resp.text().await.unwrap();
            assert_eq!(body, "hello from tunnel");
        }));
    }

    // Wait for all requests to complete
    for handle in handles {
        handle.await.expect("task should not panic");
    }
}
