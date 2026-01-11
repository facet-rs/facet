/// A calculator service with multiple operations.
pub trait Calculator {
    /// Adds two numbers together.
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Subtracts b from a.
    async fn subtract(&self, a: i32, b: i32) -> i32;

    /// Multiplies two numbers.
    async fn multiply(&self, a: i32, b: i32) -> i32;

    /// Divides a by b, returning an error if b is zero.
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String>;
}
