//! Tests for client middleware hook execution through the Caller pipeline.

use std::sync::{Arc, Mutex};

use vox::memory_link_pair;

#[vox::service]
trait Echo {
    async fn echo(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct EchoService;

impl Echo for EchoService {
    async fn echo(&self, value: u32) -> u32 {
        value
    }
}

#[derive(Clone)]
struct RecordingMiddleware {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

impl vox::ClientMiddleware for RecordingMiddleware {
    fn pre<'a, 'call>(
        &'a self,
        context: &'a vox::ClientContext<'a>,
        _request: &'a mut vox::ClientRequest<'call, 'a>,
    ) -> vox::BoxMiddlewareFuture<'a> {
        let name = self.name;
        let method_name = context.method().map(|m| m.method_name).unwrap_or("unknown");
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .unwrap()
                .push(format!("{name}:pre:{method_name}"));
        })
    }

    fn post<'a>(
        &'a self,
        context: &'a vox::ClientContext<'a>,
        outcome: vox::ClientCallOutcome<'a>,
    ) -> vox::BoxMiddlewareFuture<'a> {
        let name = self.name;
        let method_name = context.method().map(|m| m.method_name).unwrap_or("unknown");
        let ok = outcome.is_ok();
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .unwrap()
                .push(format!("{name}:post:{method_name}:{ok}"));
        })
    }
}

#[tokio::test]
async fn middleware_hooks_fire_in_order() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        s
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    let events = Arc::new(Mutex::new(Vec::new()));

    let client = client
        .with_middleware(RecordingMiddleware {
            name: "first",
            events: events.clone(),
        })
        .with_middleware(RecordingMiddleware {
            name: "second",
            events: events.clone(),
        });

    let result = client.echo(42).await.expect("echo with middleware");
    assert_eq!(result, 42);

    let events = events.lock().unwrap();
    assert_eq!(
        &*events,
        &[
            "first:pre:echo",
            "second:pre:echo",
            // post hooks run in reverse order
            "second:post:echo:true",
            "first:post:echo:true",
        ]
    );
}

struct MetadataInjectingMiddleware;

impl vox::ClientMiddleware for MetadataInjectingMiddleware {
    fn pre<'a, 'call>(
        &'a self,
        _context: &'a vox::ClientContext<'a>,
        request: &'a mut vox::ClientRequest<'call, 'a>,
    ) -> vox::BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            request.push_string_metadata(
                "x-test",
                "injected".to_string(),
                vox::MetadataFlags::NONE,
            );
        })
    }
}

#[vox::service]
trait MetadataProbe {
    #[vox::context]
    async fn check_metadata(&self) -> bool;
}

#[derive(Clone)]
struct MetadataProbeService;

impl MetadataProbe for MetadataProbeService {
    async fn check_metadata(&self, cx: &vox::RequestContext<'_>) -> bool {
        cx.metadata().iter().any(|e| {
            e.key == "x-test"
                && matches!(&e.value, vox::MetadataValue::String(s) if s == "injected")
        })
    }
}

#[tokio::test]
async fn middleware_can_inject_metadata() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(
                MetadataProbeDispatcher::new(MetadataProbeService).establish::<vox::NoopClient>(),
            )
            .await
            .expect("server establish");
        s
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<MetadataProbeClient>()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    // Without middleware — no injected metadata.
    let has_meta = client.check_metadata().await.expect("probe without mw");
    assert!(
        !has_meta,
        "should not have x-test metadata without middleware"
    );

    // With middleware — metadata injected.
    let client = client.with_middleware(MetadataInjectingMiddleware);
    let has_meta = client.check_metadata().await.expect("probe with mw");
    assert!(has_meta, "middleware should inject x-test metadata");
}
