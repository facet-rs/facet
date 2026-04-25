use std::time::Instant;

use facet_pretty::{ColorMode, PrettyPrinter};
use tracing::{debug, trace};

use crate::{
    BoxMiddlewareFuture, MetadataEntry, MetadataFlags, MetadataValue, RequestContext,
    ServerCallOutcome, ServerMiddleware, ServerRequest, ServerResponse, ServerResponseContext,
    ServerResponsePayload,
};

const DEFAULT_PAYLOAD_MAX_DEPTH: usize = 4;
const DEFAULT_PAYLOAD_MAX_CONTENT_LEN: usize = 128;
const DEFAULT_PAYLOAD_MAX_COLLECTION_LEN: usize = 8;

#[derive(Debug, Clone)]
pub struct ServerLoggingOptions {
    pub log_metadata: bool,
    pub payload_max_depth: usize,
    pub payload_max_content_len: usize,
    pub payload_max_collection_len: usize,
}

impl Default for ServerLoggingOptions {
    fn default() -> Self {
        Self {
            log_metadata: false,
            payload_max_depth: DEFAULT_PAYLOAD_MAX_DEPTH,
            payload_max_content_len: DEFAULT_PAYLOAD_MAX_CONTENT_LEN,
            payload_max_collection_len: DEFAULT_PAYLOAD_MAX_COLLECTION_LEN,
        }
    }
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

    pub fn with_payload_max_depth(mut self, payload_max_depth: usize) -> Self {
        self.options.payload_max_depth = payload_max_depth;
        self
    }

    pub fn with_payload_max_content_len(mut self, payload_max_content_len: usize) -> Self {
        self.options.payload_max_content_len = payload_max_content_len;
        self
    }

    pub fn with_payload_max_collection_len(mut self, payload_max_collection_len: usize) -> Self {
        self.options.payload_max_collection_len = payload_max_collection_len;
        self
    }

    fn payload_printer(&self) -> PrettyPrinter {
        PrettyPrinter::new()
            .with_colors(ColorMode::Never)
            .with_max_depth(self.options.payload_max_depth)
            .with_max_content_len(self.options.payload_max_content_len)
            .with_max_collection_len(self.options.payload_max_collection_len)
            .with_minimal_option_names(true)
    }

    fn format_payload(&self, payload: crate::Peek<'_, '_>) -> String {
        self.payload_printer().format_peek(payload)
    }

    fn format_postcard_bytes(&self, bytes: &[u8]) -> String {
        self.payload_printer().format(bytes)
    }
}

impl ServerMiddleware for ServerLogging {
    fn pre<'a>(&'a self, request: ServerRequest<'_>) -> BoxMiddlewareFuture<'a> {
        let context = request.context();
        context.extensions().insert(RequestStart(Instant::now()));
        let method = context.method();
        trace!(
            target: "vox::server",
            service = method.service_name,
            method = method.method_name,
            args = %self.format_payload(request.args()),
            "rpc request payload"
        );
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
        Box::pin(async {})
    }

    fn response<'a>(
        &'a self,
        context: &ServerResponseContext,
        response: ServerResponse<'_>,
    ) -> BoxMiddlewareFuture<'a> {
        let method = context.method();
        match response.payload() {
            ServerResponsePayload::Value(ret) => {
                trace!(
                    target: "vox::server",
                    service = method.service_name,
                    method = method.method_name,
                    ret = %self.format_payload(ret),
                    metadata = ?RedactedMetadata(response.metadata()),
                    "rpc response payload"
                );
            }
            ServerResponsePayload::PostcardBytes(bytes) => {
                trace!(
                    target: "vox::server",
                    service = method.service_name,
                    method = method.method_name,
                    ret = %self.format_postcard_bytes(bytes),
                    metadata = ?RedactedMetadata(response.metadata()),
                    "rpc response payload"
                );
            }
        }
        Box::pin(async {})
    }

    fn post<'a>(
        &'a self,
        context: &RequestContext<'_>,
        outcome: ServerCallOutcome,
    ) -> BoxMiddlewareFuture<'a> {
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
        Box::pin(async {})
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

    use super::{
        MetadataEntryDebug, MetadataValueDebug, RedactedMetadata, ServerLogging,
        ServerLoggingOptions,
    };
    use crate::{
        Extensions, MetadataEntry, MetadataFlags, MetadataValue, RequestContext, ServerCallOutcome,
        ServerMiddleware, ServerRequest,
    };

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
            MetadataEntry {
                key: "attempt".into(),
                value: MetadataValue::U64(2),
                flags: MetadataFlags::NONE,
            },
        ];

        assert_eq!(
            format!("{:?}", RedactedMetadata(&metadata)),
            "[MetadataEntry { key: \"authorization\", value: \"[REDACTED]\", flags: MetadataFlags(1) }, MetadataEntry { key: \"blob\", value: <3 bytes>, flags: MetadataFlags(0) }, MetadataEntry { key: \"attempt\", value: 2, flags: MetadataFlags(0) }]"
        );

        let bytes = MetadataValue::Bytes((&[0; 4][..]).into());
        assert_eq!(format!("{:?}", MetadataValueDebug(&bytes)), "<4 bytes>");
        assert_eq!(
            format!(
                "{:?}",
                MetadataEntryDebug(&MetadataEntry {
                    key: "plain".into(),
                    value: MetadataValue::String("value".into()),
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
                .with_filter(LevelFilter::TRACE),
        );
        let _guard = tracing::subscriber::set_default(subscriber);

        static METHOD: crate::MethodDescriptor = crate::MethodDescriptor {
            id: crate::MethodId(7),
            service_name: "Audit",
            method_name: "record",
            args_shape: <() as facet::Facet<'static>>::SHAPE,
            args: &[],
            return_shape: <() as facet::Facet<'static>>::SHAPE,
            args_have_channels: false,
            retry: crate::RetryPolicy::VOLATILE,
            doc: None,
        };

        let metadata = vec![
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
        ];
        let extensions = Extensions::new();
        let context = RequestContext::with_extensions(&METHOD, &metadata, &extensions);
        let request = ServerRequest::new(context, crate::Peek::new(&()));
        let response_wire: Result<i32, crate::VoxError<std::convert::Infallible>> = Ok(42);
        let response = crate::RequestResponse {
            metadata: vec![MetadataEntry {
                key: "etag".into(),
                value: MetadataValue::String("v1".into()),
                flags: MetadataFlags::NONE,
            }],
            ret: crate::Payload::outgoing(&response_wire),
            schemas: Default::default(),
        };

        let logging = ServerLogging::new(ServerLoggingOptions {
            log_metadata: true,
            ..Default::default()
        });
        logging.pre(request).await;
        logging
            .response(
                &crate::ServerResponseContext::new(
                    request.method(),
                    request.request_id(),
                    request.connection_id(),
                    request.extensions().clone(),
                ),
                crate::ServerResponse::new(&response),
            )
            .await;
        logging
            .post(request.context(), ServerCallOutcome::Replied)
            .await;

        let output = writer.output();
        assert!(output.contains("rpc request"));
        assert!(output.contains("rpc request payload"));
        assert!(output.contains("rpc response"));
        assert!(output.contains("rpc response payload"));
        assert!(output.contains("authorization"));
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("Bearer secret"));
        assert!(output.contains("attempt"));
        assert!(output.contains("42"));
        assert!(output.contains("etag"));
        assert!(output.contains("Replied"));
    }

    #[tokio::test]
    async fn server_logging_truncates_large_payloads() {
        let writer = SharedWriter::default();
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_writer(writer.clone())
                .with_filter(LevelFilter::TRACE),
        );
        let _guard = tracing::subscriber::set_default(subscriber);

        static METHOD: crate::MethodDescriptor = crate::MethodDescriptor {
            id: crate::MethodId(8),
            service_name: "Audit",
            method_name: "bulk_record",
            args_shape: <(Vec<u32>, String) as facet::Facet<'static>>::SHAPE,
            args: &[],
            return_shape: <Vec<u32> as facet::Facet<'static>>::SHAPE,
            args_have_channels: false,
            retry: crate::RetryPolicy::VOLATILE,
            doc: None,
        };

        let args = (
            vec![1u32, 2, 3, 4, 5],
            "abcdefghijklmnopqrstuvwxyz".to_string(),
        );
        let extensions = Extensions::new();
        let context = RequestContext::with_extensions(&METHOD, &[], &extensions);
        let request = ServerRequest::new(context, crate::Peek::new(&args));
        let response_wire: Result<Vec<u32>, crate::VoxError<std::convert::Infallible>> =
            Ok(vec![10, 20, 30, 40, 50]);
        let response = crate::RequestResponse {
            metadata: vec![],
            ret: crate::Payload::outgoing(&response_wire),
            schemas: Default::default(),
        };

        let logging = ServerLogging::new(ServerLoggingOptions {
            payload_max_depth: 6,
            payload_max_content_len: 8,
            payload_max_collection_len: 3,
            ..Default::default()
        });
        logging.pre(request).await;
        logging
            .response(
                &crate::ServerResponseContext::new(
                    request.method(),
                    request.request_id(),
                    request.connection_id(),
                    request.extensions().clone(),
                ),
                crate::ServerResponse::new(&response),
            )
            .await;

        let output = writer.output();
        assert!(output.contains("rpc request payload"));
        assert!(output.contains("more items"), "output: {output}");
        assert!(output.contains("chars"));
        assert!(output.contains("rpc response payload"));
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
