# Handoff: Initial Connect Waiting Policy For Autostarted Daemons

## Summary

Vox now correctly distinguishes retryable and non-retryable recovery failures during
session recovery. The remaining gap is different: there is no clear high-level
API/policy for **initial connection waiting** when the caller has just spawned a
local daemon and wants to wait for it to become reachable.

Vixen had papered over that with a custom polling loop around `Daemon::status()`.
That loop was wrong and has now been removed.

The next Vox work should define a first-class initial-connect waiting policy/API,
so callers do not invent their own startup retry loops.

## Why This Matters

The trigger was `vx eval --fresh-daemon` against a stale local daemon socket.
The newer client expected a newer `DaemonStatus` shape and got the older one.
That produced a schema translation failure:

```text
invalid vox payload: protocol error: translation plan failed: at ::Ok: required field 'pid' ... missing from remote 'DaemonStatus' ...
```

That specific failure is now correctly modeled in Vox as non-retryable.
The problem was that Vixen had its own retry loop and retried it anyway.

User direction was explicit:
- do not paper over this in Vixen
- fix Vox so retry policy lives in Vox
- use Vox semantics rather than custom client loops

## Current State In Vox

Repo state when this note was written:
- repo: `/Users/amos/bearcove/vox`
- branch: `vox-refinements`
- HEAD: `903b1759a6f12e0ce10492b5d31d7f3b413d9922`

### What Vox Already Does Correctly

1. `VoxError` retryability exists in:
- `/Users/amos/bearcove/vox/rust/vox-types/src/vox_error.rs`

Relevant behavior:
- `InvalidPayload`, `UnknownMethod`, `User`, `Cancelled`, `Indeterminate` are non-retryable
- `ConnectionClosed`, `SessionShutdown`, `SendFailed` are retryable

2. `SessionError` retryability exists in:
- `/Users/amos/bearcove/vox/rust/vox-core/src/session/mod.rs`

Relevant behavior:
- `Io`, `ConnectTimeout`, `NotResumable` are retryable
- `Protocol`, `Rejected` are non-retryable

3. The built-in recoverer now stops on non-retryable errors in:
- `/Users/amos/bearcove/vox/rust/vox-core/src/session/builders.rs`

Relevant logic:
```rust
match result {
    Ok(conduit) => return Ok(conduit),
    Err(e) if !e.is_retryable() => return Err(e),
    Err(_) => {}
}
```

4. There is an explicit schema compatibility regression test in:
- `/Users/amos/bearcove/vox/rust/vox/tests/schema_compat_tests.rs`

Relevant test:
- `missing_required_field_is_non_retryable`

That test verifies:
- translation-plan failure from a missing required field
- surfaces as `VoxError::InvalidPayload`
- `err.is_retryable() == false`

### The Remaining Gap

High-level `vox::connect(address).await` is still a **one-shot initial establish**.
It does not expose a first-class “wait for service to appear” policy.

Relevant file:
- `/Users/amos/bearcove/vox/rust/vox/src/highlevel/mod.rs`

Relevant behavior:
- parse address
- construct initiator builder
- call `.establish::<Client>().await`
- return immediately on failure

That means Vox currently has:
- good retryability semantics for recovery after a session exists
- but no clear high-level API for initial waiting when a local daemon was just spawned

## What Changed In Vixen

Repo:
- `/Users/amos/bearcove/vixen`
- branch: `daemon-consolidation`

The incorrect custom retry loop has been removed.

Commit:
- `ed97eabb` `vx: remove custom daemon connect retry`

That commit also reverts the partial schema workaround:
- `DaemonStatus.pid` is back to required `u32`
- the local compatibility paper-over is gone

So the system is now in the honest state:
- Vox recovery retryability is real
- Vixen no longer invents initial connection retry policy
- initial daemon-autostart waiting now needs a proper Vox-owned API or policy

## Exact Problem To Solve

Define how callers should express this intent:

> I have just spawned a daemon process for this address. Wait for the initial connection to succeed, but only retry failures that are actually retryable.

This is **not** the same problem as connection recovery after an existing session dies.

The missing API/policy should answer:
- how does a caller request initial waiting?
- what errors are retryable during initial waiting?
- how long does it wait?
- where does backoff live?
- how is the final failure surfaced?

## Constraints

1. Do not regress the new non-retryable semantics.
- schema incompatibility must still fail immediately
- protocol mismatch must still fail immediately

2. Do not put this policy back into application code.
- Vixen should not grow another ad hoc loop
- the point is to centralize this in Vox

3. Keep the distinction between:
- initial service appearance waiting
- session recovery

They may share retryability classification and backoff machinery, but they are not the same semantic phase.

4. Do not “solve” this by making every schema addition optional.
- the stale daemon/new client mismatch should remain a real incompatibility
- the system should simply surface it promptly and clearly

## Suggested Directions

There are at least three plausible shapes. Any of them would be better than caller-side polling.

### Option A: High-level connect builder gets an initial waiting mode

Example shape only:

```rust
vox::connect(address)
    .wait_for_service(Duration::from_secs(5))
    .connect_timeout(Duration::from_millis(200))
    .await?
```

Semantics:
- retry initial establish on retryable `SessionError`
- stop immediately on non-retryable `SessionError`
- once a session is established, ordinary session recovery semantics take over

Pros:
- ergonomic
- directly addresses the daemon autostart case

### Option B: Separate API for “establish eventually”

Example shape only:

```rust
vox::connect(address)
    .establish_with_policy(InitialConnectPolicy { ... })
    .await?
```

or

```rust
vox::wait_connect(address, policy).await?
```

Pros:
- keeps one-shot `establish()` semantics clean
- makes the distinction between one-shot connect and waiting connect explicit

### Option C: Reuse recoverer machinery for initial establish under an explicit mode

Conceptually:
- no custom app loop
- initial connect could internally reuse the same retryability-aware backoff logic as `BareSourceRecoverer`
- but exposed as a separate public behavior

Pros:
- less duplicated policy

Risk:
- easy to blur “initial connect” with “recovery after session existed” unless the public API stays explicit

## Recommendation

My recommendation is:
- keep `establish()` as one-shot
- add an explicit waiting API/policy for initial connect
- internally reuse the same retryability classification and backoff rules already present in Vox recovery

That gives clean semantics:
- one-shot connect remains simple
- waiting connect is explicit
- retryability stays Vox-owned

## Concrete Files To Inspect

Primary files:
- `/Users/amos/bearcove/vox/rust/vox/src/highlevel/mod.rs`
- `/Users/amos/bearcove/vox/rust/vox-core/src/session/builders.rs`
- `/Users/amos/bearcove/vox/rust/vox-core/src/session/mod.rs`
- `/Users/amos/bearcove/vox/rust/vox-types/src/vox_error.rs`
- `/Users/amos/bearcove/vox/rust/vox/tests/schema_compat_tests.rs`

Vixen consumer that motivated this:
- `/Users/amos/bearcove/vixen/crates/vx/src/main.rs`

## Success Criteria

1. Vox exposes a first-class initial-connect waiting mechanism or policy.
2. That mechanism retries only retryable failures.
3. Schema/protocol incompatibility still fails immediately.
4. Vixen can daemon-autostart without maintaining its own polling loop.
5. The stale-daemon/new-client mismatch produces one clear failure, not repeated reconnect churn.

## Things Not To Do

- do not reintroduce a Vixen-side poll loop
- do not weaken schema compatibility just to dodge the error
- do not collapse initial connect waiting and session recovery into one unnamed behavior
- do not treat all `SessionError` or all `VoxError` as uniformly retryable

## Quick Repro Context

The failure mode that exposed this was:
1. old daemon already listening on `local:///tmp/vixen.vox`
2. new `vx` client starts
3. client expects newer `DaemonStatus`
4. daemon responds with older `DaemonStatus`
5. payload translation fails immediately
6. caller-side retry loop churns uselessly

That final step is now removed in Vixen. The missing piece is a Vox-owned initial-connect waiting API.
