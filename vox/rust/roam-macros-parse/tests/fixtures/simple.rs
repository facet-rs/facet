trait Echo {
    async fn echo(&self, message: String) -> String;
}
