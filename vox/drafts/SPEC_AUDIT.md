# Rapace Spec Audit

This document analyzes the specification rules against the [Requirements Guidelines](requirements-guidelines.md).

## Summary

| Category | Count | Notes |
|----------|-------|-------|
| **Total rules** | 265 | |
| MUST/MUST NOT/SHALL | 210 | Normative requirements (79%) |
| SHOULD/SHOULD NOT | 26 | Recommendations (10%) |
| MAY | 9 | Optional behaviors (3%) |
| No RFC 2119 keyword | 20 | Need review (8%) |

## Action Items

### 1. Remove from normative spec (move to Deployment Guide)

These 12 SHOULD/MAY rules are **deployment/operational guidance**, not protocol behavior:

| Rule | Keyword | Issue |
|------|---------|-------|
| `observability.overhead` | SHOULD NOT | Unmeasurable ("significantly impact") |
| `observability.otel` | SHOULD | Implementation suggestion, not protocol |
| `observability.log.fields` | SHOULD | Implementation suggestion |
| `observability.sampling` | SHOULD | Implementation suggestion |
| `observability.sampling.override` | MAY | Implementation suggestion |
| `overload.detection.metrics` | SHOULD | Operational guidance |
| `overload.circuit-breaker` | SHOULD | Client implementation pattern |
| `overload.retry.backoff` | SHOULD | Client implementation pattern |
| `overload.goaway.client.backoff` | SHOULD | Client implementation pattern |
| `overload.shedding.order` | SHOULD | Server implementation pattern |
| `security.profile-a.transport` | MAY | Deployment guidance |
| `security.profile-a.unix` | SHOULD | Deployment guidance |
| `security.profile-b.isolation` | SHOULD | Deployment guidance |
| `security.profile-b.shm` | SHOULD | Deployment guidance |
| `security.profile-c.pinning` | SHOULD | Deployment guidance |
| `security.metadata.tls` | SHOULD | Deployment guidance |
| `security.metadata.tokens` | SHOULD | Deployment guidance |

**Note**: MUST rules in security profiles (e.g., `security.profile-b.authenticate`, `security.profile-c.reject`) are **kept** because they describe normative protocol requirements.

**Recommendation**: Create a new `docs/content/guide/deployment.md` and move SHOULD/MAY content there without `r[rule.id]` markers.

### 2. Add RFC 2119 keywords

These 20 rules lack RFC 2119 keywords. Each should be reviewed to determine if they're:
- **Normative** → Add MUST/MUST NOT
- **Descriptive** → Remove `r[rule.id]` marker (it's just documentation)

| Rule | Current Text (truncated) | Recommendation |
|------|--------------------------|----------------|
| `cancel.deadline.terminal` | "The DEADLINE_EXCEEDED error is terminal..." | Descriptive - remove marker |
| `cancel.deadline.exceeded` | "When now() > deadline_ns:..." | Add MUST for the behavior |
| `cancel.ordering` | "Cancellation is asynchronous..." | Descriptive - remove marker |
| `cancel.shm.reclaim` | "When a channel is canceled..." | Add MUST for cleanup |
| `core.call.complete` | "A call is complete when..." | Descriptive - remove marker |
| `core.call.error.flags` | "Errors are signaled within..." | Merge with `core.call.error.flag-match` |
| `core.call.optional-ports` | "If a stream port is optional..." | Add MUST for behavior |
| `core.call.required-port-missing` | "If a required port is never opened..." | Add MUST for error |
| `core.channel.open.attach-required` | Table of attach requirements | Merge with `core.channel.open` |
| `core.channel.open.attach-validation` | "When receiving OpenChannel..." | Add MUST for validation |
| `core.channel.open.call-validation` | "When receiving OpenChannel..." | Add MUST for validation |
| `core.channel.open.cancel-on-violation` | "All CancelChannel responses..." | Add MUST |
| `core.close.close-channel-semantics` | "Sent on channel 0..." | Descriptive - remove marker |
| `core.close.full` | "A channel is fully closed when..." | Descriptive - remove marker |
| `core.flow.eos-no-credits` | "Frames with only EOS flag..." | Add MUST NOT consume |
| `core.goaway.after-send` | "After sending GoAway..." | Add MUST for behavior |
| `core.goaway.last-channel-id` | "last_channel_id semantics..." | Descriptive - merge with existing |
| `core.method-id.zero-enforcement` | "Enforcement:..." | Merge with `core.method-id.zero-reserved` |
| `core.method-id.zero-reserved` | "method_id = 0: Reserved..." | Already has MUST elsewhere |
| `core.stream.decode-failure` | "If payload decoding fails..." | Add MUST for behavior |
| `core.stream.type-enforcement` | "The receiver knows..." | Descriptive - remove marker |
| `core.tunnel.credits` | "Credits for TUNNEL channels..." | Add MUST for counting |
| `frame.msg-id.control` | "Control channel: Control frames..." | Descriptive - remove marker |
| `handshake.explicit-required` | "Explicit handshake is..." | Redundant with `handshake.required` |
| `metadata.key.case-sensitive` | "Keys are compared as raw bytes..." | Descriptive - remove marker |
| `payload.stability.canonical` | "This document is canonical..." | Meta - remove marker |
| `payload.struct.order-immutable` | "Field order is part of schema..." | Add MUST NOT reorder |
| `priority.non-guarantee` | "Implementations are NOT required..." | Remove marker (describes non-requirements) |
| `priority.propagation.rules` | "For downstream calls..." | Needs SHOULD/MAY |
| `priority.value.range` | "Rapace uses 8-bit priority..." | Descriptive - remove marker |
| `security.auth-failure.handshake` | "If authentication fails..." | Add MUST for behavior |
| `security.metadata.plaintext` | "Hello params are NOT encrypted..." | Informational - remove marker |
| `transport.webtransport.datagram-restrictions` | "Datagrams use same format..." | Add MUST for restrictions |

### 3. Review remaining SHOULD rules for testability

These SHOULD rules describe **protocol behavior** and should stay, but verify they're testable:

| Rule | Keep? | Notes |
|------|-------|-------|
| `cancel.impl.check-deadline` | Keep | Testable: send expired deadline, verify rejection |
| `cancel.impl.error-response` | Keep | Testable: cancel, verify error response |
| `error.details.populate` | Keep | Testable: trigger error, check details |
| `langmap.idiomatic` | Remove | Not testable - style guidance |
| `metadata.limits.reject` | Keep | Testable: exceed limits, check RESOURCE_EXHAUSTED |
| `overload.drain.grace-period` | Keep | Testable: send GoAway, verify grace period |
| `priority.guarantee.ordering` | Keep | Testable: send mixed priority, check order |
| `priority.scheduling.queue` | Keep | Testable: verify queue behavior |
| `transport.backpressure` | Keep | Testable: verify backpressure signals |
| `transport.keepalive.transport` | Keep | Testable: verify keepalive behavior |

### 4. Review MAY rules

MAY rules grant permission and don't need coverage. These are fine as-is:

- `cancel.impl.ignore-data` - Permission to ignore
- `core.flags.high-priority` - Optional optimization
- `core.flow.credit-additive` - Clarification
- `core.flow.infinite-credit` - Optional mode
- `core.stream.empty` - Clarification of encoding
- `data.type-system.additional` - Extension point
- `error.impl.custom-codes` - Extension point
- `schema.collision.runtime` - Defensive behavior

## Estimated Impact

After cleanup:
- **Remove** ~17 SHOULD/MAY guidance rules from coverage requirements
- **Keep** all MUST rules as normative requirements
- **Fix** ~15 rules missing keywords (some become descriptive text)

Target coverage: **100% of MUST/MUST NOT rules** (not SHOULD/MAY)
