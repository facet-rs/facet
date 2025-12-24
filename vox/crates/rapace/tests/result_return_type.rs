//! Regression test for issue #55: Result<T, E> as return type
//! https://github.com/bearcove/rapace/issues/55

use rapace::{RpcSession, Transport};
use std::sync::Arc;

#[rapace::service]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String>;
}

struct MyCalculator;
impl Calculator for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a + b)
    }

    async fn divide(&self, a: i32, b: i32) -> Result<i32, String> {
        if b == 0 {
            Err("division by zero".to_string())
        } else {
            Ok(a / b)
        }
    }
}

#[tokio_test_lite::test]
async fn test_result_ok_variant() {
    let (client_transport, server_transport) = Transport::mem_pair();

    let server = tokio::spawn(CalculatorServer::new(MyCalculator).serve(server_transport));

    let session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(session.clone().run());
    let client = CalculatorClient::new(session);

    // Test successful Result::Ok
    let result = client.add(1, 2).await.unwrap();
    assert_eq!(result, Ok(3));

    server.abort();
}

#[tokio_test_lite::test]
async fn test_result_err_variant() {
    let (client_transport, server_transport) = Transport::mem_pair();

    let server = tokio::spawn(CalculatorServer::new(MyCalculator).serve(server_transport));

    let session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(session.clone().run());
    let client = CalculatorClient::new(session);

    // Test Result::Err from division by zero
    let result = client.divide(10, 0).await.unwrap();
    assert_eq!(result, Err("division by zero".to_string()));

    server.abort();
}

#[tokio_test_lite::test]
async fn test_result_ok_with_computation() {
    let (client_transport, server_transport) = Transport::mem_pair();

    let server = tokio::spawn(CalculatorServer::new(MyCalculator).serve(server_transport));

    let session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(session.clone().run());
    let client = CalculatorClient::new(session);

    // Test successful division
    let result = client.divide(10, 2).await.unwrap();
    assert_eq!(result, Ok(5));

    server.abort();
}
