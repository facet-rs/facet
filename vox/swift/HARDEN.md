# Roam Swift Hardening Guide

This guide defines non-optional hardening rules for Swift runtime code under `swift/`.
The goal is to prevent crashy boundary bugs, opaque transport failures, and silent
concurrency regressions.

## 1. Treat Boundary Inputs as Untrusted

Rules:
- Validate every transport/frame/metadata input before use.
- Never assume foreign/system APIs uphold Swift non-nullability at runtime.
- Map boundary failures to precise errors; do not collapse into catch-all buckets.

Required behavior:
- Validate frame size, payload size, and negotiated limits before sending.
- On malformed/truncated/invalid frame input, fail with explicit decode/protocol errors.
- When a connection is terminally broken, fail all pending requests immediately.

## 2. Error Taxonomy Must Be Precise

Transport-level errors must use specific categories:
- `wouldBlock`: backpressure only, recoverable.
- `connectionClosed`: peer closed/dead.
- `frameEncoding` / `frameDecoding`: local encode/decode problems.
- `protocolViolation`: negotiated/protocol invariants violated.
- `transportIO`: underlying SHM/socket/doorbell I/O errors.

Rules:
- Do not use misleading names (for example, labeling I/O failures as decode errors).
- Include operation context in logs (`send`, `recv`, `doorbell wait`, request id).

## 3. Observability Is Part of Correctness

Required logs:
- Transport reader failures must include reason and trigger fail-fast teardown.
- Request timeout must log request id and timeout value.
- Connection terminal transitions must be logged once with cause.

Rules:
- Never silently convert terminal errors to EOF without logging.
- If run loop exits unexpectedly, fail pending calls and close transport.

## 4. Unsafe/Concurrency Policy

There is no single Swift equivalent of Rust `#![forbid(unsafe_code)]`, so enforce by
policy + automated checks:

- No new `@unchecked Sendable` without written justification.
- No new `@preconcurrency import` without justification.
- No new `Unmanaged`, `Unsafe*`, or `withUnsafe*` outside approved low-level files.
- Keep unsafe code in tiny wrappers with comments describing invariants.
- Prefer actors, `Sendable`, and strict concurrency-safe APIs.

## 5. Mechanical Enforcement

Run:

```bash
cd swift/roam-runtime
./scripts/check-hardening.sh
```

This script fails if unsafe/concurrency escape hatches are introduced outside the
current allowlist.

## 6. Implementation Checklist for New Code

- Boundary input validated
- Precise error category selected
- Timeout/terminal path logged
- Pending requests cleaned up on terminal failures
- Hardening check script passes
