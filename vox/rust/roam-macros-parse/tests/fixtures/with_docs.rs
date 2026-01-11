/// A simple echo service that echoes messages back.
trait Echo {
    /// Echoes the message back to the caller.
    async fn echo(&self, message: String) -> String;
}
