#![deny(unsafe_code)]

use rapace_service_macros::service;

/// Simple echo service for conformance testing.
#[service]
pub trait Echo {
    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;
}

pub fn all_services() -> Vec<rapace_schema::ServiceDetail> {
    vec![echo_service_detail()]
}

