use std::time::Instant;

use tracing::debug;

use crate::{
    BoxMiddlewareFuture, ClientCallOutcome, ClientContext, ClientMiddleware, ClientRequest,
    Metadata, MetadataExt, metadata_key_is_redacted,
};

#[derive(Debug, Clone, Default)]
pub struct ClientLoggingOptions {
    pub log_metadata: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ClientLogging {
    options: ClientLoggingOptions,
}

impl ClientLogging {
    pub fn new(options: ClientLoggingOptions) -> Self {
        Self { options }
    }

    pub fn with_metadata(mut self, log_metadata: bool) -> Self {
        self.options.log_metadata = log_metadata;
        self
    }
}

impl ClientMiddleware for ClientLogging {
    fn pre<'a, 'call>(
        &'a self,
        context: &'a ClientContext<'a>,
        request: &'a mut ClientRequest<'call, 'a>,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            context.extensions().insert(RequestStart(Instant::now()));
            let method = context.method();
            if self.options.log_metadata {
                debug!(
                    target: "vox::client",
                    service = method.map(|method| method.service_name),
                    method = method.map(|method| method.method_name),
                    method_id = %context.method_id(),
                    metadata = ?RedactedMetadata(request.metadata()),
                    "rpc request"
                );
            } else {
                debug!(
                    target: "vox::client",
                    service = method.map(|method| method.service_name),
                    method = method.map(|method| method.method_name),
                    method_id = %context.method_id(),
                    "rpc request"
                );
            }
        })
    }

    fn post<'a>(
        &'a self,
        context: &'a ClientContext<'a>,
        outcome: ClientCallOutcome<'a>,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            let method = context.method();
            let duration_ms = context
                .extensions()
                .with::<RequestStart, _>(|start| start.0.elapsed().as_secs_f64() * 1_000.0);
            match outcome {
                ClientCallOutcome::Response => {
                    debug!(
                        target: "vox::client",
                        service = method.map(|method| method.service_name),
                        method = method.map(|method| method.method_name),
                        method_id = %context.method_id(),
                        duration_ms,
                        outcome = "response",
                        "rpc response"
                    );
                }
                ClientCallOutcome::Error(error) => {
                    debug!(
                        target: "vox::client",
                        service = method.map(|method| method.service_name),
                        method = method.map(|method| method.method_name),
                        method_id = %context.method_id(),
                        duration_ms,
                        error = ?error,
                        outcome = "error",
                        "rpc response"
                    );
                }
            }
        })
    }
}

#[derive(Debug)]
struct RequestStart(Instant);

/// Debug view of metadata that redacts the values of keys marked sensitive by
/// the metadata sigil convention.
#[derive(Clone, Copy)]
pub(crate) struct RedactedMetadata<'a>(pub(crate) &'a Metadata);

impl std::fmt::Debug for RedactedMetadata<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (key, value) in self.0.meta_entries() {
            if metadata_key_is_redacted(key) {
                map.entry(&key, &"[REDACTED]");
            } else {
                map.entry(&key, &MetadataValueDebug(value));
            }
        }
        map.finish()
    }
}

struct MetadataValueDebug<'a>(&'a Metadata);

impl std::fmt::Debug for MetadataValueDebug<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(s) = self.0.as_string() {
            s.as_str().fmt(f)
        } else if let Some(b) = self.0.as_bytes() {
            write!(f, "<{} bytes>", b.as_slice().len())
        } else if let Some(n) = self.0.as_number().and_then(|n| n.to_u64()) {
            n.fmt(f)
        } else {
            write!(f, "{:?}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use tracing_subscriber::{
        Layer,
        filter::LevelFilter,
        fmt::{self, MakeWriter},
        layer::SubscriberExt,
    };

    use super::{ClientLogging, ClientLoggingOptions, RedactedMetadata};
    use crate::{
        LaneAcceptor, LaneRequest, Metadata, MethodDescriptor, MethodId, PendingLane, meta_set,
        metadata,
    };

    #[derive(Clone)]
    struct LoggingTestClient {
        caller: crate::Caller,
    }

    impl crate::FromVoxLane for LoggingTestClient {
        const SERVICE_NAME: &'static str = "Audit";

        fn from_vox_lane(
            caller: crate::Caller,
            _connection: Option<crate::ConnectionHandle>,
        ) -> Self {
            Self { caller }
        }
    }

    struct AuditLaneAcceptor;

    impl LaneAcceptor for AuditLaneAcceptor {
        fn accept(&self, request: &LaneRequest, lane: PendingLane) -> Result<(), Metadata> {
            assert_eq!(request.service(), "Audit");
            lane.handle_with(());
            Ok(())
        }
    }

    // r[verify rpc.metadata.sigils]
    #[test]
    fn metadata_debug_redacts_sensitive_values() {
        let mut m = metadata().bytes("blob", &[1u8, 2, 3][..]).build();
        meta_set(&mut m, "#authorization", "Bearer secret");

        let rendered = format!("{:?}", RedactedMetadata(&m));
        assert!(
            rendered.contains("\"#authorization\": \"[REDACTED]\""),
            "{rendered}"
        );
        assert!(rendered.contains("\"blob\": <3 bytes>"), "{rendered}");
        assert!(!rendered.contains("Bearer secret"), "{rendered}");
    }

    #[tokio::test]
    async fn client_logging_emits_redacted_request_and_response_logs() {
        use vox_core::testing::breakable_link_pair;

        let writer = SharedWriter::default();
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_writer(writer.clone())
                .with_filter(LevelFilter::DEBUG),
        );
        let _guard = tracing::subscriber::set_default(subscriber);

        // Set up a real in-memory connection, open the Audit service lane, and
        // then close the server side so the client caller returns SendFailed.
        let (link_a, _break_a, link_b, break_b) = breakable_link_pair(16);

        let server = tokio::spawn(async move {
            let connection = crate::acceptor_on(link_b)
                .on_connection(AuditLaneAcceptor)
                .establish_connection()
                .await
                .expect("server establish");
            connection.closed().await;
        });

        // Client side: establish with the logging middleware
        let caller = crate::initiator_on(link_a)
            .establish::<LoggingTestClient>()
            .await
            .expect("client establish");

        // Close the link so the client gets an error on next call.
        break_b.close().await;
        server.await.expect("server task");

        // Build a client with logging middleware
        static METHOD: MethodDescriptor =
            MethodDescriptor {
                id: MethodId(7),
                service_name: "Audit",
                method_name: "record",
                args_shape: <() as facet::Facet<'static>>::SHAPE,
                args: &[],
                return_shape: <() as facet::Facet<'static>>::SHAPE,
                response_wire_shape:
                    <Result<(), crate::VoxError<std::convert::Infallible>> as facet::Facet<
                        'static,
                    >>::SHAPE,
                args_have_channels: false,
                doc: None,
            };

        static SERVICE: crate::ServiceDescriptor = crate::ServiceDescriptor {
            service_name: "Audit",
            methods: &[&METHOD],
            doc: None,
        };

        let logging = ClientLogging::new(ClientLoggingOptions { log_metadata: true });
        let caller = caller.caller.with_middleware(&SERVICE, logging);

        let mut request_metadata = metadata().u64("attempt", 2).build();
        meta_set(&mut request_metadata, "#authorization", "Bearer secret");
        let _ = caller
            .call(crate::RequestCall {
                method_id: MethodId(7),
                channels: Vec::new(),
                metadata: request_metadata,
                args: crate::Payload::Encoded(&[]),
                schemas: Default::default(),
            })
            .await;

        let output = writer.output();
        assert!(
            output.contains("rpc request"),
            "expected 'rpc request' in output: {output}"
        );
        assert!(
            output.contains("rpc response"),
            "expected 'rpc response' in output: {output}"
        );
        assert!(
            output.contains("#authorization"),
            "expected '#authorization' in output: {output}"
        );
        assert!(
            output.contains("[REDACTED]"),
            "expected '[REDACTED]' in output: {output}"
        );
        assert!(
            !output.contains("Bearer secret"),
            "expected no 'Bearer secret' in output: {output}"
        );
        assert!(
            output.contains("attempt"),
            "expected 'attempt' in output: {output}"
        );
    }

    #[derive(Clone, Default)]
    struct SharedWriter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl SharedWriter {
        fn output(&self) -> String {
            let bytes = self.output.lock().expect("shared writer mutex poisoned");
            String::from_utf8(bytes.clone()).expect("log output should be utf-8")
        }
    }

    impl<'a> MakeWriter<'a> for SharedWriter {
        type Writer = SharedWriterGuard;

        fn make_writer(&'a self) -> Self::Writer {
            SharedWriterGuard {
                output: Arc::clone(&self.output),
            }
        }
    }

    struct SharedWriterGuard {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl io::Write for SharedWriterGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output
                .lock()
                .expect("shared writer mutex poisoned")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
