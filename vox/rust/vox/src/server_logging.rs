use std::time::Instant;

use tracing::debug;

use crate::{
    BoxMiddlewareFuture, MetadataEntry, MetadataFlags, MetadataValue, RequestContext,
    ServerCallOutcome, ServerMiddleware,
};

#[derive(Debug, Clone, Default)]
pub struct ServerLoggingOptions {
    pub log_metadata: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ServerLogging {
    options: ServerLoggingOptions,
}

impl ServerLogging {
    pub fn new(options: ServerLoggingOptions) -> Self {
        Self { options }
    }

    pub fn with_metadata(mut self, log_metadata: bool) -> Self {
        self.options.log_metadata = log_metadata;
        self
    }
}

impl ServerMiddleware for ServerLogging {
    fn pre<'a>(&'a self, context: &'a RequestContext<'a>) -> BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            context.extensions().insert(RequestStart(Instant::now()));
            let method = context.method();
            if self.options.log_metadata {
                debug!(
                    target: "vox::server",
                    service = method.service_name,
                    method = method.method_name,
                    metadata = ?RedactedMetadata(context.metadata()),
                    "rpc request"
                );
            } else {
                debug!(
                    target: "vox::server",
                    service = method.service_name,
                    method = method.method_name,
                    "rpc request"
                );
            }
        })
    }

    fn post<'a>(
        &'a self,
        context: &'a RequestContext<'a>,
        outcome: ServerCallOutcome,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            let method = context.method();
            let duration_ms = context
                .extensions()
                .with::<RequestStart, _>(|start| start.0.elapsed().as_secs_f64() * 1_000.0);
            debug!(
                target: "vox::server",
                service = method.service_name,
                method = method.method_name,
                outcome = ?outcome,
                duration_ms,
                "rpc response"
            );
        })
    }
}

#[derive(Debug)]
struct RequestStart(Instant);

#[derive(Clone, Copy)]
struct RedactedMetadata<'a>(&'a [MetadataEntry<'static>]);

impl std::fmt::Debug for RedactedMetadata<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut entries = f.debug_list();
        for entry in self.0 {
            entries.entry(&MetadataEntryDebug(entry));
        }
        entries.finish()
    }
}

struct MetadataEntryDebug<'a>(&'a MetadataEntry<'static>);

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

    use super::{
        MetadataEntryDebug, MetadataValueDebug, RedactedMetadata, ServerLogging,
        ServerLoggingOptions,
    };
    use crate::{
        Extensions, MetadataEntry, MetadataFlags, MetadataValue, RequestContext, ServerCallOutcome,
        ServerMiddleware,
    };

    #[test]
    fn metadata_debug_redacts_sensitive_values() {
        let metadata = vec![
            MetadataEntry {
                key: "authorization",
                value: MetadataValue::String("Bearer secret"),
                flags: MetadataFlags::SENSITIVE,
            },
            MetadataEntry {
                key: "blob",
                value: MetadataValue::Bytes(&[1, 2, 3]),
                flags: MetadataFlags::NONE,
            },
            MetadataEntry {
                key: "attempt",
                value: MetadataValue::U64(2),
                flags: MetadataFlags::NONE,
            },
        ];

        assert_eq!(
            format!("{:?}", RedactedMetadata(&metadata)),
            "[MetadataEntry { key: \"authorization\", value: \"[REDACTED]\", flags: MetadataFlags(1) }, MetadataEntry { key: \"blob\", value: <3 bytes>, flags: MetadataFlags(0) }, MetadataEntry { key: \"attempt\", value: 2, flags: MetadataFlags(0) }]"
        );

        let bytes = MetadataValue::Bytes(&[0; 4]);
        assert_eq!(format!("{:?}", MetadataValueDebug(&bytes)), "<4 bytes>");
        assert_eq!(
            format!(
                "{:?}",
                MetadataEntryDebug(&MetadataEntry {
                    key: "plain",
                    value: MetadataValue::String("value"),
                    flags: MetadataFlags::NONE,
                })
            ),
            "MetadataEntry { key: \"plain\", value: \"value\", flags: MetadataFlags(0) }"
        );
    }

    #[tokio::test]
    async fn server_logging_emits_redacted_request_and_response_logs() {
        let writer = SharedWriter::default();
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_writer(writer.clone())
                .with_filter(LevelFilter::DEBUG),
        );
        let _guard = tracing::subscriber::set_default(subscriber);

        static METHOD: crate::MethodDescriptor = crate::MethodDescriptor {
            id: crate::MethodId(7),
            service_name: "Audit",
            method_name: "record",
            args_shape: <() as facet::Facet<'static>>::SHAPE,
            args: &[],
            return_shape: <() as facet::Facet<'static>>::SHAPE,
            retry: crate::RetryPolicy::VOLATILE,
            doc: None,
        };

        let metadata = vec![
            MetadataEntry {
                key: "authorization",
                value: MetadataValue::String("Bearer secret"),
                flags: MetadataFlags::SENSITIVE,
            },
            MetadataEntry {
                key: "attempt",
                value: MetadataValue::U64(2),
                flags: MetadataFlags::NONE,
            },
        ];
        let extensions = Extensions::new();
        let context = RequestContext::with_extensions(&METHOD, &metadata, &extensions);

        let logging = ServerLogging::new(ServerLoggingOptions { log_metadata: true });
        logging.pre(&context).await;
        logging.post(&context, ServerCallOutcome::Replied).await;

        let output = writer.output();
        assert!(output.contains("rpc request"));
        assert!(output.contains("rpc response"));
        assert!(output.contains("authorization"));
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("Bearer secret"));
        assert!(output.contains("attempt"));
        assert!(output.contains("Replied"));
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
