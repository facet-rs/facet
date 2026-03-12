+++
title = "Retry"
description = "Retry safety and semantics in roam"
weight = 13
+++

The retry layer defines how roam handles ambiguous failures: cases where the
client does not know whether the server received, started, completed, or lost
an operation. It sits above transport/session continuity and below application
logic.

# The fundamental ambiguity

After any communication failure, the client faces irreducible uncertainty.
The previous attempt is in one of these conditions, and the client cannot
distinguish them:

  1. The request never left the client's outbound buffer
  2. The request arrived but the handler never started
  3. The handler started and is still running
  4. The handler completed but the response was lost in transit
  5. The handler started and then failed, with or without partial side effects

Any retry model must handle all five as possible realities behind a single
"unknown" from the client's perspective.

# Operation identity

> r[retry.op-id]
>
> Every RPC is bound to an **operation ID**: a client-generated identifier that
> names one logical operation across multiple delivery attempts.

> r[retry.op-id.uniqueness]
>
> The client MUST mint a unique operation ID for each logical operation. Every
> delivery attempt for that operation carries the same ID. A new intention,
> even with identical arguments, gets a new ID.

> r[retry.op-id.scope]
>
> Operation IDs are scoped to a session. When a session ends cleanly, operation
> records for that session may be evicted.

> r[retry.op-id.payload-binding]
>
> If the same operation ID arrives with a different method or different
> serialized arguments, the server MUST reject it as a conflict.

# Operation state machine

The server maintains a record mapping operation IDs to states:

```
              ┌──────────────┐
              │   Absent     │  ← no record exists
              └──────┬───────┘
                     │ first attempt arrives
                     ▼
              ┌──────────────┐
         ┌────│    Live      │────┐
         │    └──────┬───────┘    │
         │           │            │
   release/cancel    │ handler    │ crash / lost state
   (volatile only)   │ returns    │ before a recoverable
         │           │            │ terminal outcome
         ▼           ▼            ▼
  ┌──────────┐  ┌──────────┐  ┌──────────────┐
  │ Released │  │ Sealed   │  │Indeterminate │
  └──────────┘  └──────────┘  └──────────────┘
```

> r[retry.state.absent]
>
> **Absent.** No record of this operation ID. A new attempt triggers normal
> admission and transitions to Live.

> r[retry.state.live]
>
> **Live.** The operation has been admitted and is not yet terminal. There
> MUST be at most one live execution owner for any operation ID.

> r[retry.state.released]
>
> **Released.** The runtime has relinquished responsibility for the operation
> without a sealed outcome. This state is only valid for volatile methods.

> r[retry.state.sealed]
>
> **Sealed(outcome).** A terminal outcome has been recorded. Retries observe
> that same outcome rather than creating a second independent execution.

> r[retry.state.indeterminate]
>
> **Indeterminate.** The server lost enough state that it cannot prove whether
> the logical operation reached a terminal outcome.

# Static method retry policy

Retry behavior is fixed at admission time. There is no handler-visible
mid-flight `commit()` point. Instead, each method has a static retry policy
described by two dimensions.

## Volatile vs persist

**Volatile** is the default. A volatile method may be released on client drop,
disconnect, timeout, or cancellation. Once released, the server is no longer
responsible for carrying that logical operation to completion.

**Persist** is the opposite of volatile. A persist method becomes sticky from
the instant it is admitted. The dangerous point is treated as immediate on
invocation.

> r[retry.policy.volatile.default]
>
> A method without `persist` is volatile.

> r[retry.policy.persist]
>
> A method declared `persist` is non-volatile. Once admitted, the runtime MUST
> NOT release it merely because the caller dropped interest, disconnected, or
> sent a cancellation request.

## Idem vs non-idem

`idem` declares that re-executing the same logical operation is semantically
safe. `idem` is orthogonal to `persist`.

> r[retry.policy.idem]
>
> A method declared `idem` may be re-executed for the same logical operation
> when the retry state machine permits it.

> r[retry.policy.non-idem.default]
>
> A method without `idem` is non-idem. The runtime MUST NOT re-execute the same
> logical operation unless some stronger proof outside this spec makes it safe.

## The four combinations

The two dimensions produce four static method classes:

1. **volatile + non-idem**: best-effort operations. They may be released. Same
   operation ID is not transparently rerunnable.
2. **volatile + idem**: re-executable operations. They may be released and
   later re-run under the same operation ID.
3. **persist + non-idem**: sticky operations. Once admitted, they must continue
   toward a terminal outcome or an honest indeterminate failure.
4. **persist + idem**: sticky and rerunnable operations. They are not released,
   but if they reach Indeterminate, re-execution remains safe.

# Duplicate attempt handling

> r[retry.duplicate.absent]
>
> If the operation is Absent, the server admits the attempt and starts the
> operation.

> r[retry.duplicate.live]
>
> If the operation is Live, the server MUST NOT start a second independent
> execution owner for the same operation ID.

> r[retry.duplicate.live.attach]
>
> A duplicate arriving while Live MUST attach to the existing live operation and
> observe the same eventual resolution.

> r[retry.duplicate.sealed]
>
> If the operation is Sealed, the server MUST replay the sealed terminal
> outcome.

> r[retry.duplicate.released.idem]
>
> If the operation is Released and the method is `idem`, the runtime MAY treat
> the retry as a fresh execution of the same logical operation.

> r[retry.duplicate.released.non-idem]
>
> If the operation is Released and the method is not `idem`, the runtime MUST
> fail closed. It MUST NOT silently turn the same operation ID into a fresh
> re-execution.

> r[retry.duplicate.indeterminate.idem]
>
> If the operation is Indeterminate and the method is `idem`, the runtime MAY
> re-execute the operation under the same operation ID.

> r[retry.duplicate.indeterminate.non-idem]
>
> If the operation is Indeterminate and the method is not `idem`, the runtime
> MUST fail closed unless it can recover a sealed outcome from durable state.

# Sealed outcomes

Returning from the handler seals the operation.

> r[retry.seal.return]
>
> A handler return produces the terminal sealed outcome for that logical
> operation.

> r[retry.seal.terminal-replay]
>
> A sealed failure MUST be replayed on retry. A retry of the same logical
> operation MUST NOT turn a sealed failure into a second independent attempt.

> r[retry.seal.absorbing]
>
> Sealed is absorbing. Once an operation is Sealed, later cancel, drop, or
> retry attempts MUST NOT unseal it.

# Cancellation and dropped interest

Cancellation is an event in the retry state machine, not rollback magic.

> r[retry.cancel.explicit.volatile]
>
> For a volatile method, an explicit cancellation request MAY release the
> operation.

> r[retry.cancel.explicit.persist]
>
> For a persist method, explicit cancellation does not authorize release. The
> operation remains Live until it seals or becomes Indeterminate.

> r[retry.cancel.implicit.volatile]
>
> For a volatile method, disconnect or dropped interest MAY release the
> operation.

> r[retry.cancel.implicit.persist]
>
> For a persist method, disconnect or dropped interest detaches the observer but
> does not release the operation.

> r[retry.cancel.race]
>
> Cancellation races with sealing. Whichever state transition wins first
> determines the result observed by later retries.

# Session resumption and retry

Transport/session continuity can hide some failures, but it does not define
logical operation semantics.

> r[retry.reconnect.stable-conduit]
>
> StableConduit continuity is below the retry layer. It may hide a conduit
> break, but it does not by itself authorize re-executing an operation.

> r[retry.reconnect.session-resume]
>
> Session resumption keeps operation identity alive across a conduit break. When
> a resumed session cannot prove the outcome of an in-flight operation, retry is
> expressed by sending the same operation ID again.

> r[retry.layers.no-silent-retry]
>
> Lower transport/session layers MAY retransmit bytes they know were not
> delivered. They MUST NOT silently re-execute RPC operations once delivery has
> become ambiguous.

# Operation record lifetime

> r[retry.gc.ttl]
>
> Operation records MUST outlive the maximum retry window by a comfortable
> margin.

> r[retry.gc.live-protected]
>
> A Live operation MUST NOT be evicted.

> r[retry.gc.fail-closed]
>
> If the runtime can recognize that an operation ID has expired, it MUST reject
> the retry rather than treating the ID as Absent.

# Channels and retry

Channels are connection-bound and do not compose with retry the same way a
plain request/response pair does.

> r[retry.channel.connection-bound]
>
> Channel bindings are attached to a concrete connection, not directly to an
> operation ID.

> r[retry.channel.persist-forbidden]
>
> A method with channel arguments MUST NOT be declared `persist` until the
> runtime defines channel continuity for sticky operations.

> r[retry.channel.volatile.rebinding]
>
> When an `idem` volatile method with channels is re-executed on retry, the
> runtime MAY rebind fresh channel handles for the new execution attempt.

# Summary

The retry contract is split across three parties:

- **The caller** mints an operation ID and reuses it when addressing the same
  logical operation again.
- **The runtime** tracks operation state, honors method retry policy, prevents
  duplicate live execution owners, and replays sealed outcomes.
- **The method declaration** tells the runtime whether the operation is
  volatile or persist, and whether same-operation re-execution is safe.
