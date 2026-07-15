//! The `Primitive` trait and its effect vocabulary: tickets, completions, and
//! the `EffectCtx` witness window.
//!
//! r[machine.primitive.trait] — a primitive begins an effect non-blockingly and
//! reports its outcome as a [`Completion`].
//! r[machine.primitive.effectctx-witness-only] — the ONLY machine-plane window a
//! primitive gets is [`EffectCtx`]: it may witness reads, emit events, intern
//! through the store, and report exactly one completion. Nothing else.
//!
//! Phase 02 lands this trait layer ahead of its consumers: the scheduler wires
//! `begin`/`EffectCtx`/tickets in phase 05. The items are exercised by unit
//! tests here; `dead_code` is expected until that wiring exists.
#![allow(dead_code)]

use crate::runtime::identity::{DemandKey, ValueId};
use crate::runtime::model::{ReadWitness, Receipt};
use crate::runtime::store::{FrozenValue, Interned, Store};

use super::descriptor::{PrimitiveDescriptor, PrimitiveId};

/// Request handed to a primitive: the interned identity plus the frozen tree.
pub(crate) struct RequestRef<'a> {
    pub identity: ValueId,
    pub frozen: &'a FrozenValue,
}

/// One in-flight effect. Owned by the DEMAND, never the task
/// (r[machine.scheduler.tickets-outlive-tasks]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EffectTicket(pub u64);

/// A typed failure a primitive reports as a LANGUAGE result (it memoizes under
/// the primitive's policy). v1 carries a response-independent rendered code +
/// message pair; the typed per-primitive failure schema axis is reserved for a
/// later phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveFailure {
    pub code: String,
    pub message: String,
}

/// The outcome a primitive reports through [`EffectCtx::complete`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Completion {
    Ok(Interned),
    Failed(PrimitiveFailure),
}

/// A machine-plane event a primitive emits for observability. It never enters a
/// value identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectEvent {
    pub primitive: PrimitiveId,
    pub message: String,
}

/// An effect-protocol violation by a registered primitive. A misbehaving
/// registered primitive is EXPECTED fallibility, surfaced as this typed error
/// rather than a panic (`machine.error.typed`). Phase 05 lifts this into a
/// dedicated `RuntimeFault` variant on the shared machine-error plane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectProtocolError {
    /// `begin` returned without reporting any completion.
    CompletionMissing,
    /// `complete` was called more than once for one effect.
    CompletionAlreadyReported,
    /// The request tree did not match the registered request schema.
    RequestShape { message: String },
}

impl std::fmt::Display for EffectProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CompletionMissing => f.write_str("primitive reported no completion"),
            Self::CompletionAlreadyReported => {
                f.write_str("primitive reported more than one completion")
            }
            Self::RequestShape { message } => write!(f, "request shape mismatch: {message}"),
        }
    }
}

impl std::error::Error for EffectProtocolError {}

/// The primitive's ONLY machine window (r[machine.primitive.effectctx-witness-only]).
pub(crate) struct EffectCtx<'a> {
    store: &'a mut Store,
    witnessed: Vec<ReadWitness>,
    completion: Option<Completion>,
    completion_calls: usize,
    events: Vec<EffectEvent>,
}

impl<'a> EffectCtx<'a> {
    pub(crate) fn new(store: &'a mut Store) -> Self {
        Self {
            store,
            witnessed: Vec::new(),
            completion: None,
            completion_calls: 0,
            events: Vec::new(),
        }
    }

    /// Record a read the effect observed, so the demand's receipt carries it.
    pub(crate) fn witness_read(&mut self, source: ValueId, projection: &str) {
        self.witnessed.push(ReadWitness {
            source,
            projection: projection.to_owned(),
        });
    }

    /// Emit a machine-plane observability event.
    pub(crate) fn emit(&mut self, event: EffectEvent) {
        self.events.push(event);
    }

    /// Report the effect's outcome. A second call is not a panic: it is recorded
    /// and surfaced by [`finish`](Self::finish) as a typed protocol error.
    pub(crate) fn complete(&mut self, completion: Completion) {
        self.completion_calls += 1;
        if self.completion.is_none() {
            self.completion = Some(completion);
        }
    }

    /// The store window primitives intern responses through. Not visible to
    /// primitive impls outside the crate.
    pub(crate) fn store_mut(&mut self) -> &mut Store {
        self.store
    }

    /// Consume the ctx and produce the effect's outcome, receipt, and events.
    /// The scheduler (phase 05) is the only caller. A missing or duplicated
    /// completion is a typed protocol violation, not a panic.
    pub(crate) fn finish(
        self,
        demand: DemandKey,
    ) -> Result<(Completion, Receipt, Vec<EffectEvent>), EffectProtocolError> {
        match self.completion_calls {
            0 => Err(EffectProtocolError::CompletionMissing),
            1 => {
                let completion = self
                    .completion
                    .expect("exactly one completion was recorded");
                let receipt = Receipt {
                    demand,
                    reads: self.witnessed,
                };
                Ok((completion, receipt, self.events))
            }
            _ => Err(EffectProtocolError::CompletionAlreadyReported),
        }
    }
}

/// A registered Rust effect primitive.
///
/// r[machine.primitive.trait]
pub(crate) trait Primitive: Send + Sync {
    fn descriptor(&self) -> &PrimitiveDescriptor;

    /// Non-blocking begin (r[machine.primitive.trait]). v1 adapters complete
    /// inline before returning; the signature already permits async backends.
    fn begin(
        &self,
        request: RequestRef<'_>,
        ctx: &mut EffectCtx<'_>,
    ) -> Result<EffectTicket, EffectProtocolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::identity::{Digest, SchemaId};

    fn demand_key() -> DemandKey {
        DemandKey(Digest([7u8; 32]))
    }

    fn sample_value() -> ValueId {
        ValueId {
            schema: SchemaId::named("vix.test.source"),
            content: Digest([3u8; 32]),
        }
    }

    #[test]
    fn happy_path_yields_receipt_with_reads_and_demand() {
        let mut store = Store::default();
        let mut ctx = EffectCtx::new(&mut store);
        ctx.witness_read(sample_value(), "field");
        let interned = ctx
            .store_mut()
            .intern_realized(SchemaId::named("vix.test.result"), b"result");
        ctx.complete(Completion::Ok(interned));
        let (completion, receipt, events) = ctx.finish(demand_key()).unwrap();
        assert!(matches!(completion, Completion::Ok(_)));
        assert_eq!(receipt.demand, demand_key());
        assert_eq!(receipt.reads.len(), 1);
        assert_eq!(receipt.reads[0].source, sample_value());
        assert_eq!(receipt.reads[0].projection, "field");
        assert!(events.is_empty());
    }

    #[test]
    fn no_completion_is_a_typed_error() {
        let mut store = Store::default();
        let ctx = EffectCtx::new(&mut store);
        assert_eq!(
            ctx.finish(demand_key()),
            Err(EffectProtocolError::CompletionMissing)
        );
    }

    #[test]
    fn double_completion_is_a_typed_error() {
        let mut store = Store::default();
        let mut ctx = EffectCtx::new(&mut store);
        let first = ctx
            .store_mut()
            .intern_realized(SchemaId::named("vix.test.a"), b"a");
        let second = ctx
            .store_mut()
            .intern_realized(SchemaId::named("vix.test.b"), b"b");
        ctx.complete(Completion::Ok(first));
        ctx.complete(Completion::Ok(second));
        assert_eq!(
            ctx.finish(demand_key()),
            Err(EffectProtocolError::CompletionAlreadyReported)
        );
    }
}
