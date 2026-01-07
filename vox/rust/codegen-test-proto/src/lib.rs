//! Test proto crate for validating build.rs codegen approach.

use roam::service;
use roam::session::{Pull, Push};

/// Simple calculator service for testing codegen.
#[service]
pub trait Calculator {
    /// Add two numbers.
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Multiply two numbers.
    async fn multiply(&self, a: i32, b: i32) -> i32;

    /// Sum a stream of numbers.
    async fn sum_stream(&self, numbers: Pull<i32>) -> i64;

    /// Generate a range of numbers.
    async fn range(&self, count: u32) -> Push<u32>;
}

/// Returns the service detail for build.rs access.
pub fn service_detail() -> roam::schema::ServiceDetail {
    calculator_service_detail()
}
