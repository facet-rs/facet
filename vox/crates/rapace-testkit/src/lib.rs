//! rapace-testkit: Conformance test suite for rapace transports.
//!
//! Provides `TransportFactory` trait and shared test scenarios that all
//! transports must pass.
//!
//! # Usage
//!
//! Each transport crate implements `TransportFactory` and runs the shared tests:
//!
//! ```ignore
//! struct MyTransportFactory;
//!
//! impl TransportFactory for MyTransportFactory {
//!     type T = MyTransport;
//!     async fn connect_pair() -> Result<(Self::T, Self::T), TestError> {
//!         // create connected pair
//!     }
//! }
//!
//! #[tokio::test]
//! async fn my_transport_unary_happy_path() {
//!     rapace_testkit::run_unary_happy_path::<MyTransportFactory>().await;
//! }
//! ```

// TODO: implement TransportFactory trait and test scenarios
