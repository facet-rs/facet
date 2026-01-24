pub trait LifetimeService {
    async fn process(&self, data: Patch<'static>);
}
