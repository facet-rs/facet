+++
title = "Annex A: Requirements Guidelines"
description = "Guidelines for writing and evaluating specification requirements"
weight = 200
+++

This annex describes the principles used to write requirements in this specification. These guidelines are drawn from safety-critical systems engineering practice and ensure that requirements are traceable, testable, and unambiguous.

## Scope of Normative Requirements

Normative requirements (marked with `r[rule.id]`) describe **what conforming software does**, not how it should be deployed or operated.

Requirements should describe:

- Wire format and encoding
- Message semantics and state machines
- Error conditions and responses
- Timing constraints (where measurable)

Requirements should NOT describe:

- Deployment recommendations (use TLS, use Unix sockets)
- Operational guidance (monitor these metrics, use circuit breakers)
- Implementation suggestions (use this algorithm, use this data structure)

Non-normative guidance belongs in separate sections or documents (e.g., "Deployment Guide", "Best Practices") and should not use the `r[rule.id]` marker.

## Characteristics of Good Requirements

Each requirement should be:

### Specific

A requirement should make a single claim. It is better to have five requirements that each claim one thing than one requirement that claims five things.

**Avoid:**
> The server MUST validate the request, check authorization, and respond within 100ms.

**Prefer:**
> r[request.validation] The server MUST validate the request format before processing.
>
> r[request.authorization] The server MUST verify authorization before executing the request.
>
> r[request.timeout] The server MUST respond within 100ms of receiving the request.

### Measurable

The success criteria must be clear and objective. Avoid subjective or relative terms.

**Avoid:**
> The implementation SHOULD respond as quickly as possible.
>
> The system SHOULD NOT significantly impact performance.

**Prefer:**
> The implementation MUST respond within 500ms.
>
> Instrumentation overhead MUST NOT exceed 1% of baseline latency at p99.

### Positive

Requirements should state what the system does, not what it avoids. Negative requirements are harder to test exhaustively.

**Avoid:**
> The system MUST NOT lag when processing input.

**Prefer:**
> The system MUST process each input event within 20ms.

### Well-Formed

Requirements use RFC 2119 keywords (MUST, SHOULD, MAY) precisely. Combined with the "Specific" principle, this usually means each requirement contains exactly one RFC 2119 keyword.

| Keyword | Meaning |
|---------|---------|
| MUST / MUST NOT | Absolute requirement. Non-compliance is a protocol violation. |
| SHOULD / SHOULD NOT | Recommended. Exceptions require justification. |
| MAY | Truly optional. No expectation either way. |

### Traceable

Every requirement should trace in three directions:

1. **Upward**: To a higher-level requirement or design goal (except top-level requirements)
2. **Downward**: To a lower-level requirement or implementation
3. **Sideways**: To a validation test case

This specification uses [tracey](https://github.com/bearcove/tracey) markers to maintain traceability:

```rust
// [impl rule.id]: Implementation of the requirement
// [verify rule.id]: Test that validates the requirement
// [depends rule.id]: This code depends on the rule being satisfied
```

## Evaluating Existing Requirements

When reviewing requirements, ask:

1. **Is it testable?** Can you write a conformance test that passes or fails based on this requirement?
2. **Is it specific?** Does it make exactly one claim?
3. **Is it about behavior?** Does it describe what software does, or how humans should deploy it?
4. **Is it measurable?** Are the success criteria objective?

If a requirement fails these tests, consider:

- Splitting it into multiple specific requirements
- Adding concrete thresholds or conditions
- Moving it to non-normative guidance
- Removing it entirely if it adds no value

## References

For more on requirements engineering in safety-critical systems:

- NASA Systems Engineering Handbook (NASA/SP-2016-6105)
- DO-178C: Software Considerations in Airborne Systems
- ISO 26262: Road Vehicles - Functional Safety
- IEEE 830: Recommended Practice for Software Requirements Specifications
