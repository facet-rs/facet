//! Connection state machine and message loop.
//!
//! Handles the protocol state machine including Hello exchange,
//! payload validation, and stream ID management.

use std::sync::Arc;
use std::time::Duration;

use roam_session::{OutgoingPoll, Role, StreamError, StreamIdAllocator, StreamRegistry};
use roam_wire::{Hello, Message};
use tokio::sync::Notify;

use crate::framing::CobsFramed;

/// Negotiated connection parameters after Hello exchange.
#[derive(Debug, Clone)]
pub struct Negotiated {
    /// Effective max payload size (min of both peers).
    pub max_payload_size: u32,
    /// Initial stream credit (min of both peers).
    pub initial_credit: u32,
}

/// Error during connection handling.
///
/// r[impl core.error.connection] - Connection errors are unrecoverable protocol violations
#[derive(Debug)]
pub enum ConnectionError {
    /// IO error.
    Io(std::io::Error),
    /// Protocol violation requiring Goodbye.
    ProtocolViolation {
        /// Rule ID that was violated.
        rule_id: &'static str,
        /// Human-readable context.
        context: String,
    },
    /// Dispatch error.
    Dispatch(String),
    /// Connection closed cleanly.
    Closed,
}

impl From<std::io::Error> for ConnectionError {
    fn from(e: std::io::Error) -> Self {
        ConnectionError::Io(e)
    }
}

/// A live connection with completed Hello exchange.
pub struct Connection {
    io: CobsFramed,
    role: Role,
    negotiated: Negotiated,
    stream_allocator: StreamIdAllocator,
    stream_registry: StreamRegistry,
    #[allow(dead_code)]
    our_hello: Hello,
}

impl Connection {
    /// Get a mutable reference to the underlying framed IO.
    pub fn io(&mut self) -> &mut CobsFramed {
        &mut self.io
    }

    /// Get the negotiated parameters.
    pub fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }

    /// Get the connection role.
    pub fn role(&self) -> Role {
        self.role
    }

    /// Get the stream ID allocator.
    ///
    /// r[impl streaming.allocation.caller] - Caller allocates ALL stream IDs.
    pub fn stream_allocator(&self) -> &StreamIdAllocator {
        &self.stream_allocator
    }

    /// Send a Goodbye message and return an error.
    ///
    /// r[impl message.goodbye.send] - Send Goodbye with rule ID before closing.
    /// r[impl core.error.goodbye-reason] - Reason contains violated rule ID.
    pub async fn goodbye(&mut self, rule_id: &'static str) -> ConnectionError {
        let _ = self
            .io
            .send(&Message::Goodbye {
                reason: rule_id.into(),
            })
            .await;
        ConnectionError::ProtocolViolation {
            rule_id,
            context: String::new(),
        }
    }

    /// Validate a stream ID according to protocol rules.
    ///
    /// Returns the rule ID if validation fails.
    pub fn validate_stream_id(&self, stream_id: u64) -> Result<(), &'static str> {
        // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
        if stream_id == 0 {
            return Err("streaming.id.zero-reserved");
        }

        // r[impl streaming.unknown] - Unknown stream IDs are connection errors.
        if !self.stream_registry.contains(stream_id) {
            return Err("streaming.unknown");
        }

        Ok(())
    }

    /// Get a mutable reference to the stream registry.
    ///
    /// Used by dispatchers to register streams before processing requests.
    pub fn stream_registry_mut(&mut self) -> &mut StreamRegistry {
        &mut self.stream_registry
    }

    /// Get the notify handle for outgoing stream data.
    ///
    /// When an `OutgoingSender` has new data, it notifies this handle.
    /// Use in select! to wake up when stream data is ready to send.
    pub fn outgoing_notify(&self) -> Arc<Notify> {
        self.stream_registry.outgoing_notify()
    }

    /// Send all pending outgoing stream messages.
    ///
    /// Drains the outgoing stream channels and sends Data/Close messages
    /// to the peer. Call this periodically or after processing requests.
    ///
    /// r[impl streaming.data] - Send Data messages for outgoing streams.
    /// r[impl streaming.close] - Send Close messages when streams end.
    pub async fn flush_outgoing(&mut self) -> Result<(), ConnectionError> {
        loop {
            match self.stream_registry.poll_outgoing() {
                OutgoingPoll::Data { stream_id, payload } => {
                    let msg = Message::Data { stream_id, payload };
                    self.io.send(&msg).await?;
                }
                OutgoingPoll::Close { stream_id } => {
                    let msg = Message::Close { stream_id };
                    self.io.send(&msg).await?;
                }
                OutgoingPoll::Pending | OutgoingPoll::Done => {
                    // No more pending data
                    break;
                }
            }
        }
        Ok(())
    }

    /// Validate payload size against negotiated limit.
    ///
    /// r[impl flow.unary.payload-limit] - Payloads bounded by max_payload_size.
    /// r[impl message.hello.negotiation] - Effective limit is min of both peers.
    pub fn validate_payload_size(&self, size: usize) -> Result<(), &'static str> {
        if size as u32 > self.negotiated.max_payload_size {
            return Err("flow.unary.payload-limit");
        }
        Ok(())
    }

    /// Run the message loop with a dispatcher.
    ///
    /// This is the main event loop that:
    /// - Receives messages from the peer
    /// - Validates them according to protocol rules
    /// - Dispatches requests to the service
    /// - Sends responses back
    /// - Flushes outgoing stream data when notified
    ///
    /// r[impl unary.pipelining.allowed] - Handle requests as they arrive.
    /// r[impl unary.pipelining.independence] - Each request handled independently.
    pub async fn run<D>(&mut self, dispatcher: &D) -> Result<(), ConnectionError>
    where
        D: ServiceDispatcher,
    {
        // Get notify handle before entering loop - OutgoingSenders will notify
        // when they have data ready to send.
        let outgoing_notify = self.stream_registry.outgoing_notify();

        loop {
            tokio::select! {
                biased;

                // Prioritize incoming messages over outgoing flush
                result = self.io.recv_timeout(Duration::from_secs(30)) => {
                    let msg = match result {
                        Ok(Some(m)) => m,
                        Ok(None) => return Ok(()),
                        Err(e) => {
                            // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
                            // Check for unknown Hello variant: [Message::Hello=0][Hello::unknown=1+]
                            // The test crafts [0x00, 0x01] = Message::Hello(0) + Hello::<variant 1>
                            // which fails postcard parsing because only variant 0 (V1) exists.
                            let raw = &self.io.last_decoded;
                            if raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00 {
                                return Err(self.goodbye("message.hello.unknown-version").await);
                            }
                            return Err(ConnectionError::Io(e));
                        }
                    };

                    match self.handle_message(msg, dispatcher).await {
                        Ok(()) => {}
                        Err(ConnectionError::Closed) => return Ok(()), // Clean shutdown
                        Err(e) => return Err(e),
                    }
                }

                // Wake up when outgoing stream data is available
                _ = outgoing_notify.notified() => {
                    self.flush_outgoing().await?;
                }
            }
        }
    }

    /// Handle a single incoming message.
    async fn handle_message<D>(
        &mut self,
        msg: Message,
        dispatcher: &D,
    ) -> Result<(), ConnectionError>
    where
        D: ServiceDispatcher,
    {
        match msg {
            Message::Hello(_) => {
                // Duplicate Hello after exchange is a protocol error.
                // For now, just ignore it.
            }
            Message::Goodbye { .. } => {
                return Err(ConnectionError::Closed);
            }
            Message::Request {
                request_id,
                method_id,
                metadata: _,
                payload,
            } => {
                // r[impl flow.unary.payload-limit]
                if let Err(rule_id) = self.validate_payload_size(payload.len()) {
                    return Err(self.goodbye(rule_id).await);
                }

                // Dispatch to service - use streaming dispatch if method has Push/Pull args
                let response_payload = if dispatcher.is_streaming(method_id) {
                    dispatcher
                        .dispatch_streaming(method_id, &payload, &mut self.stream_registry)
                        .await
                        .map_err(ConnectionError::Dispatch)?
                } else {
                    dispatcher
                        .dispatch_unary(method_id, &payload)
                        .await
                        .map_err(ConnectionError::Dispatch)?
                };

                // r[impl core.call] - Callee sends Response for caller's Request.
                // r[impl core.call.request-id] - Response has same request_id.
                // r[impl unary.complete] - Send Response with matching request_id.
                // r[impl unary.lifecycle.single-response] - Exactly one Response per Request.
                let resp = Message::Response {
                    request_id,
                    metadata: Vec::new(),
                    payload: response_payload,
                };
                self.io.send(&resp).await?;

                // Flush any outgoing stream data that handlers may have queued
                self.flush_outgoing().await?;
            }
            Message::Response { .. } | Message::Cancel { .. } => {
                // Server doesn't expect these in basic mode.
            }
            Message::Data { stream_id, payload } => {
                // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
                if stream_id == 0 {
                    return Err(self.goodbye("streaming.id.zero-reserved").await);
                }

                // r[impl streaming.data] - Route Data to registered stream.
                match self.stream_registry.route_data(stream_id, payload).await {
                    Ok(()) => {}
                    Err(StreamError::Unknown) => {
                        // r[impl streaming.unknown] - Unknown stream ID.
                        return Err(self.goodbye("streaming.unknown").await);
                    }
                    Err(StreamError::DataAfterClose) => {
                        // r[impl streaming.data-after-close] - Data after Close is error.
                        return Err(self.goodbye("streaming.data-after-close").await);
                    }
                }
            }
            Message::Close { stream_id } => {
                // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
                if stream_id == 0 {
                    return Err(self.goodbye("streaming.id.zero-reserved").await);
                }

                // r[impl streaming.close] - Close the stream.
                if !self.stream_registry.contains(stream_id) {
                    return Err(self.goodbye("streaming.unknown").await);
                }
                self.stream_registry.close(stream_id);
            }
            Message::Reset { stream_id } => {
                // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
                if stream_id == 0 {
                    return Err(self.goodbye("streaming.id.zero-reserved").await);
                }

                // r[impl streaming.reset] - Forcefully terminate stream.
                // For now, treat same as Close.
                // TODO: Signal error to Pull<T> instead of clean close.
                if !self.stream_registry.contains(stream_id) {
                    return Err(self.goodbye("streaming.unknown").await);
                }
                self.stream_registry.close(stream_id);
            }
            Message::Credit { stream_id, .. } => {
                // r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
                if stream_id == 0 {
                    return Err(self.goodbye("streaming.id.zero-reserved").await);
                }

                // TODO: Implement flow control.
                // For now, validate stream exists but ignore credit.
                if !self.stream_registry.contains(stream_id) {
                    return Err(self.goodbye("streaming.unknown").await);
                }
            }
        }
        Ok(())
    }
}

/// Trait for dispatching requests to a service.
pub trait ServiceDispatcher: Send + Sync {
    /// Check if a method uses streaming (Push/Pull arguments).
    ///
    /// Returns true if the method has any streaming arguments that require
    /// channel setup before dispatch.
    fn is_streaming(&self, method_id: u64) -> bool;

    /// Dispatch a unary request and return the response payload.
    ///
    /// The dispatcher is responsible for:
    /// - Looking up the method by method_id
    /// - Deserializing arguments from payload
    /// - Calling the service method
    /// - Serializing the response
    fn dispatch_unary(
        &self,
        method_id: u64,
        payload: &[u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send;

    /// Dispatch a streaming request and return the response payload.
    ///
    /// For streaming methods, the dispatcher must:
    /// - Decode stream IDs from the payload
    /// - Register streams with the registry (incoming for Push args, outgoing for Pull args)
    /// - Create Push/Pull handles from the registry
    /// - Call the handler method with those handles
    /// - Serialize the response
    ///
    /// Returns a boxed future since each streaming method may have different async block types.
    ///
    /// r[impl streaming.allocation.caller] - Stream IDs are decoded from payload (caller allocated).
    fn dispatch_streaming(
        &self,
        method_id: u64,
        payload: &[u8],
        registry: &mut StreamRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send + '_>>;
}

/// Perform Hello exchange as the acceptor.
///
/// r[impl message.hello.timing] - Send Hello immediately after connection.
/// r[impl message.hello.ordering] - Hello sent before any other message.
pub async fn hello_exchange_acceptor(
    mut io: CobsFramed,
    our_hello: Hello,
) -> Result<Connection, ConnectionError> {
    // Send our Hello immediately
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello
    // TODO: make timeout configurable instead of hardcoded 5s
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await? {
        Some(Message::Hello(h)) => h,
        Some(_) => {
            // Received non-Hello before Hello exchange completed
            let _ = io
                .send(&Message::Goodbye {
                    reason: "message.hello.ordering".into(),
                })
                .await;
            return Err(ConnectionError::ProtocolViolation {
                rule_id: "message.hello.ordering",
                context: "received non-Hello before Hello exchange".into(),
            });
        }
        None => return Err(ConnectionError::Closed),
    };

    // r[impl message.hello.negotiation] - Effective limit is min of both peers.
    let (our_max, our_credit) = match &our_hello {
        Hello::V1 {
            max_payload_size,
            initial_stream_credit,
        } => (*max_payload_size, *initial_stream_credit),
    };
    let (peer_max, peer_credit) = match &peer_hello {
        Hello::V1 {
            max_payload_size,
            initial_stream_credit,
        } => (*max_payload_size, *initial_stream_credit),
    };

    let negotiated = Negotiated {
        max_payload_size: our_max.min(peer_max),
        initial_credit: our_credit.min(peer_credit),
    };

    Ok(Connection {
        io,
        role: Role::Acceptor,
        negotiated,
        stream_allocator: StreamIdAllocator::new(Role::Acceptor),
        stream_registry: StreamRegistry::new(),
        our_hello,
    })
}

/// Perform Hello exchange as the initiator.
///
/// r[impl message.hello.timing] - Send Hello immediately after connection.
/// r[impl message.hello.ordering] - Hello sent before any other message.
pub async fn hello_exchange_initiator(
    mut io: CobsFramed,
    our_hello: Hello,
) -> Result<Connection, ConnectionError> {
    // Send our Hello immediately
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello
    // TODO: make timeout configurable instead of hardcoded 5s
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await {
        Ok(Some(Message::Hello(h))) => h,
        Ok(Some(_)) => {
            let _ = io
                .send(&Message::Goodbye {
                    reason: "message.hello.ordering".into(),
                })
                .await;
            return Err(ConnectionError::ProtocolViolation {
                rule_id: "message.hello.ordering",
                context: "received non-Hello before Hello exchange".into(),
            });
        }
        Ok(None) => return Err(ConnectionError::Closed),
        Err(e) => {
            // r[impl message.hello.unknown-version] - Reject unknown Hello versions.
            // Check for unknown Hello variant: [Message::Hello=0][Hello::unknown=1+]
            let raw = &io.last_decoded;
            if raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00 {
                let _ = io
                    .send(&Message::Goodbye {
                        reason: "message.hello.unknown-version".into(),
                    })
                    .await;
                return Err(ConnectionError::ProtocolViolation {
                    rule_id: "message.hello.unknown-version",
                    context: "unknown Hello variant".into(),
                });
            }
            return Err(ConnectionError::Io(e));
        }
    };

    let (our_max, our_credit) = match &our_hello {
        Hello::V1 {
            max_payload_size,
            initial_stream_credit,
        } => (*max_payload_size, *initial_stream_credit),
    };
    let (peer_max, peer_credit) = match &peer_hello {
        Hello::V1 {
            max_payload_size,
            initial_stream_credit,
        } => (*max_payload_size, *initial_stream_credit),
    };

    let negotiated = Negotiated {
        max_payload_size: our_max.min(peer_max),
        initial_credit: our_credit.min(peer_credit),
    };

    Ok(Connection {
        io,
        role: Role::Initiator,
        negotiated,
        stream_allocator: StreamIdAllocator::new(Role::Initiator),
        stream_registry: StreamRegistry::new(),
        our_hello,
    })
}
