//! Test proto crate for validating build.rs codegen approach.

use roam::service;

/// Simple calculator service for testing codegen.
#[service]
pub trait Calculator {
    /// Add two numbers.
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Multiply two numbers.
    async fn multiply(&self, a: i32, b: i32) -> i32;
}

/// Returns the service detail for build.rs access.
pub fn service_detail() -> roam::schema::ServiceDetail {
    calculator_service_detail()
}
