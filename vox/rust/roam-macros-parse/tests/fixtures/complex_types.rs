/// A service demonstrating complex type handling.
pub trait ComplexTypes {
    /// Method with generic container types.
    async fn with_generics(&self, items: Vec<Option<String>>) -> Vec<u8>;

    /// Method with nested generic types.
    async fn nested_generics(
        &self,
        data: HashMap<String, Vec<i32>>,
    ) -> Option<HashMap<String, bool>>;

    /// Method with tuple types.
    async fn with_tuples(&self, pair: (i32, String)) -> (bool, Vec<u8>);

    /// Method with reference types.
    async fn with_refs(&self, data: &str) -> &'static str;

    /// Method with mutable reference in return type.
    async fn with_lifetime(&self, input: &'a str) -> &'a str;

    /// Method with fully qualified paths.
    async fn with_paths(&self, path: std::path::PathBuf) -> std::io::Result<String>;

    /// Method with no arguments besides self.
    async fn no_args(&self) -> u64;

    /// Method with no return type (unit).
    async fn no_return(&self, value: i32);

    /// Method with Result containing complex types.
    async fn complex_result(
        &self,
        input: Vec<String>,
    ) -> Result<HashMap<String, Vec<u8>>, std::io::Error>;
}
