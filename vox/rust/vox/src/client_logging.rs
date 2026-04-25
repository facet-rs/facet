use std::time::Instant;

use tracing::debug;

use crate::{
    BoxMiddlewareFuture, ClientCallOutcome, ClientContext, ClientMiddleware, ClientRequest,
    MetadataEntry, MetadataFlags, MetadataValue,
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

#[derive(Clone, Copy)]
struct RedactedMetadata<'a>(&'a [MetadataEntry<'a>]);

impl std::fmt::Debug for RedactedMetadata<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut entries = f.debug_list();
        for entry in self.0 {
            entries.entry(&MetadataEntryDebug(entry));
        }
        entries.finish()
    }
}

struct MetadataEntryDebug<'a>(&'a MetadataEntry<'a>);

impl std::fmt::Debug for MetadataEntryDebug<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entry = self.0;
        let mut debug = f.debug_struct("MetadataEntry");
        debug.field("key", &entry.key);
        if entry.flags.contains(MetadataFlags::SENSITIVE) {
            debug.field("value", &"[REDACTED]");
        } else {
            debug.field("value", &MetadataValueDebug(&entry.value));
        }
        debug.field("flags", &entry.flags);
        debug.finish()
    }
}

struct MetadataValueDebug<'a>(&'a MetadataValue<'a>);

impl std::fmt::Debug for MetadataValueDebug<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            MetadataValue::String(value) => value.fmt(f),
            MetadataValue::Bytes(bytes) => write!(f, "<{} bytes>", bytes.len()),
            MetadataValue::U64(value) => value.fmt(f),
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
    use crate::{MetadataEntry, MetadataFlags, MetadataValue, MethodDescriptor, MethodId};

    #[test]
    fn metadata_debug_redacts_sensitive_values() {
        let metadata = vec![
            MetadataEntry {
                key: "authorization".into(),
                value: MetadataValue::String("Bearer secret".into()),
                flags: MetadataFlags::SENSITIVE,
            },
            MetadataEntry {
                key: "blob".into(),
                value: MetadataValue::Bytes((&[1, 2, 3][..]).into()),
                flags: MetadataFlags::NONE,
            },
        ];

        assert_eq!(
            format!("{:?}", RedactedMetadata(&metadata)),
            "[MetadataEntry { key: \"authorization\", value: \"[REDACTED]\", flags: MetadataFlags(1) }, MetadataEntry { key: \"blob\", value: <3 bytes>, flags: MetadataFlags(0) }]"
        );
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

        // Set up a real in-memory connection and immediately close the server
        // side so the client caller returns SendFailed.
        let (link_a, _break_a, link_b, break_b) = breakable_link_pair(16);

        // Server side: establish and immediately close
        let server = tokio::spawn(async move {
            let _caller = crate::acceptor_on(link_b)
                .establish::<crate::NoopClient>()
                .await
                .expect("server establish");
            // Close the link so the client gets an error on next call
            break_b.close().await;
        });

        // Client side: establish with the logging middleware
        let caller = crate::initiator_on(link_a, crate::TransportMode::Bare)
            .establish::<crate::NoopClient>()
            .await
            .expect("client establish");

        server.await.expect("server task");

        // Build a client with logging middleware
        static METHOD: MethodDescriptor = MethodDescriptor {
            id: MethodId(7),
            service_name: "Audit",
            method_name: "record",
            args_shape: <() as facet::Facet<'static>>::SHAPE,
            args: &[],
            return_shape: <() as facet::Facet<'static>>::SHAPE,
            args_have_channels: false,
            retry: crate::RetryPolicy::VOLATILE,
            doc: None,
        };

        static SERVICE: crate::ServiceDescriptor = crate::ServiceDescriptor {
            service_name: "Audit",
            methods: &[&METHOD],
            doc: None,
        };

        let logging = ClientLogging::new(ClientLoggingOptions { log_metadata: true });
        let caller = caller.caller.with_middleware(&SERVICE, logging);

        let _ = caller
            .call(crate::RequestCall {
                method_id: MethodId(7),
                metadata: vec![
                    MetadataEntry {
                        key: "authorization".into(),
                        value: MetadataValue::String("Bearer secret".into()),
                        flags: MetadataFlags::SENSITIVE,
                    },
                    MetadataEntry {
                        key: "attempt".into(),
                        value: MetadataValue::U64(2),
                        flags: MetadataFlags::NONE,
                    },
                ],
                args: crate::Payload::PostcardBytes(&[]),
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
            output.contains("authorization"),
            "expected 'authorization' in output: {output}"
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
