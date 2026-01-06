#![deny(unsafe_code)]

use rapace::service;

/// Simple echo service for conformance testing.
#[service]
pub trait Echo {
    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;
}

pub fn all_services() -> Vec<rapace::schema::ServiceDetail> {
    vec![echo_service_detail()]
}
