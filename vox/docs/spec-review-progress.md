# Spec Review Progress

## Highest-Priority Fixes (Interoperability / Implementability)

- [x] 1. SHM vs core handshake mismatch
- [x] 2. Multi-stream binding not implementable
- [x] 3. Stream ID reuse contradiction
- [x] 4. Schema evolution contradictions
- [x] 5. SHM metadata limit (stale 1MB)
- [x] 6. Heartbeat clock inconsistency

## Underspecified (Will Cause Divergent Implementations)

- [x] 7. Hello negotiation enforcement (max_payload_size exceeded)
- [x] 8. Unknown/invalid message handling
- [x] 9. Metadata semantics (duplicates, key length, total size, order)
- [x] 10. Stream element size bounds
- [x] 11. Multi-stream stream-ID association (fixed in #2)
- [x] 12. SHM ring layout at ring_offset
- [x] 13. SHM MsgDesc flags field
- [x] 14. SHM inline payload fields (fixed in #13)
- [x] 15. SHM Goodbye payload encoding
- [x] 16. Rust spec varint encoding
- [x] 17. Rust spec u64 endianness and signature canonicalization

## Over-specified / Normativity Issues

- [x] 18. Cancel "MUST still wait for Response"
- [x] 19. Monotonic counter for request IDs
- [x] 20. "MUST use transport stream 0" (fixed in #2)

## Not Normative But Should Be

- [x] 21. Message transport binding rules need rule IDs
- [x] 22. Enforcement rules for exceeding negotiated limits (fixed in #7, #10)
- [x] 23. Stream ID reuse after Close/Reset in core (fixed in #3)

## Transport/Language Feasibility

- [x] 24. Multi-stream transport stream identity assumption (fixed in #2)
- [x] 25. SHM architecture assumptions (endianness, atomics)
- [x] 26. host_goodbye atomicity (fixed in #15)

## Missing Rationale / Examples

- [ ] 27. Wire walkthrough examples
- [ ] 28. Rationale gaps
