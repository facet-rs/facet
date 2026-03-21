use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use crate::{ReplySink, RequestContext, RequestResponse};

/// Per-request type-indexed storage shared across middleware hooks and handlers.
#[derive(Debug, Default)]
pub struct Extensions {
    inner: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl Extensions {
    /// Create a new empty extensions bag.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a typed value into the bag, returning the previous value of the same type.
    pub fn insert<T>(&self, value: T) -> Option<T>
    where
        T: Send + Sync + 'static,
    {
        let previous = self
            .inner
            .lock()
            .expect("extensions mutex poisoned")
            .insert(TypeId::of::<T>(), Box::new(value));
        previous
            .map(|boxed| {
                boxed
                    .downcast::<T>()
                    .expect("extensions type id and boxed value disagreed")
            })
            .map(|boxed| *boxed)
    }

    /// Returns `true` if a value of type `T` is present.
    pub fn contains<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        self.inner
            .lock()
            .expect("extensions mutex poisoned")
            .contains_key(&TypeId::of::<T>())
    }

    /// Borrow a typed value from the bag for the duration of `f`.
    pub fn with<T, R>(&self, f: impl FnOnce(&T) -> R) -> Option<R>
    where
        T: Send + Sync + 'static,
    {
        let guard = self.inner.lock().expect("extensions mutex poisoned");
        let value = guard.get(&TypeId::of::<T>())?;
        let value = value
            .downcast_ref::<T>()
            .expect("extensions type id and boxed value disagreed");
        Some(f(value))
    }

    /// Mutably borrow a typed value from the bag for the duration of `f`.
    pub fn with_mut<T, R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R>
    where
        T: Send + Sync + 'static,
    {
        let mut guard = self.inner.lock().expect("extensions mutex poisoned");
        let value = guard.get_mut(&TypeId::of::<T>())?;
        let value = value
            .downcast_mut::<T>()
            .expect("extensions type id and boxed value disagreed");
        Some(f(value))
    }

    /// Clone a typed value from the bag.
    pub fn get_cloned<T>(&self) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.with(|value: &T| value.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub type BoxMiddlewareFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxMiddlewareFuture<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

/// Outcome observed by server middleware after handler dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCallOutcome {
    /// The handler sent a reply through the reply sink.
    Replied,
    /// The handler returned without replying; the runtime will synthesize cancellation.
    DroppedWithoutReply,
}

impl ServerCallOutcome {
    pub fn replied(self) -> bool {
        matches!(self, Self::Replied)
    }
}

/// Observe inbound server requests before and after dispatch.
pub trait ServerMiddleware: Send + Sync + 'static {
    fn pre<'a>(&'a self, _context: &'a RequestContext<'a>) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }

    fn post<'a>(
        &'a self,
        _context: &'a RequestContext<'a>,
        _outcome: ServerCallOutcome,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }
}

#[derive(Clone)]
#[doc(hidden)]
pub struct ServerCallOutcomeHandle {
    outcome: Arc<Mutex<ServerCallOutcome>>,
}

impl ServerCallOutcomeHandle {
    pub fn outcome(&self) -> ServerCallOutcome {
        *self
            .outcome
            .lock()
            .expect("server call outcome mutex poisoned")
    }
}

#[doc(hidden)]
pub struct ObservedReplySink<R> {
    inner: Option<R>,
    outcome: ServerCallOutcomeHandle,
}

#[doc(hidden)]
pub fn observe_reply<R>(reply: R) -> (ObservedReplySink<R>, ServerCallOutcomeHandle) {
    let outcome = ServerCallOutcomeHandle {
        outcome: Arc::new(Mutex::new(ServerCallOutcome::DroppedWithoutReply)),
    };
    (
        ObservedReplySink {
            inner: Some(reply),
            outcome: outcome.clone(),
        },
        outcome,
    )
}

impl<R> ReplySink for ObservedReplySink<R>
where
    R: ReplySink,
{
    async fn send_reply(mut self, response: RequestResponse<'_>) {
        *self
            .outcome
            .outcome
            .lock()
            .expect("server call outcome mutex poisoned") = ServerCallOutcome::Replied;
        let reply = self
            .inner
            .take()
            .expect("observed reply sink can only reply once");
        reply.send_reply(response).await;
    }

    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        self.inner.as_ref().and_then(|reply| reply.channel_binder())
    }
}

#[cfg(test)]
mod tests {
    use super::{Extensions, ServerCallOutcome};

    #[test]
    fn extensions_store_values_by_type() {
        let extensions = Extensions::new();
        assert!(!extensions.contains::<u32>());
        assert_eq!(extensions.insert(41_u32), None);
        assert!(extensions.contains::<u32>());
        assert_eq!(extensions.get_cloned::<u32>(), Some(41));
        let updated = extensions.with_mut::<u32, _>(|value| {
            *value += 1;
            *value
        });
        assert_eq!(updated, Some(42));
        assert_eq!(extensions.get_cloned::<u32>(), Some(42));
    }

    #[test]
    fn server_call_outcome_reports_reply_state() {
        assert!(ServerCallOutcome::Replied.replied());
        assert!(!ServerCallOutcome::DroppedWithoutReply.replied());
    }
}
