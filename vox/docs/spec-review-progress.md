# Spec Review Progress

## Highest-Priority Fixes (Interoperability / Implementability)

- [x] 1. SHM vs core handshake mismatch
- [ ] 2. Multi-stream binding not implementable
- [ ] 3. Stream ID reuse contradiction
- [ ] 4. Schema evolution contradictions
- [ ] 5. SHM metadata limit (stale 1MB)
- [ ] 6. Heartbeat clock inconsistency

## Underspecified (Will Cause Divergent Implementations)

- [ ] 7. Hello negotiation enforcement (max_payload_size exceeded)
- [ ] 8. Unknown/invalid message handling
- [ ] 9. Metadata semantics (duplicates, key length, total size, order)
- [ ] 10. Stream element size bounds
- [ ] 11. Multi-stream stream-ID association
- [ ] 12. SHM ring layout at ring_offset
- [ ] 13. SHM MsgDesc flags field
- [ ] 14. SHM inline payload fields (payload_offset/payload_generation)
- [ ] 15. SHM Goodbye payload encoding
- [ ] 16. Rust spec varint encoding
- [ ] 17. Rust spec u64 endianness and signature canonicalization

## Over-specified / Normativity Issues

- [ ] 18. Cancel "MUST still wait for Response"
- [ ] 19. Monotonic counter for request IDs
- [ ] 20. "MUST use transport stream 0"

## Not Normative But Should Be

- [ ] 21. Message transport binding rules need rule IDs
- [ ] 22. Enforcement rules for exceeding negotiated limits
- [ ] 23. Stream ID reuse after Close/Reset in core

## Transport/Language Feasibility

- [ ] 24. Multi-stream transport stream identity assumption
- [ ] 25. SHM architecture assumptions (endianness, atomics)
- [ ] 26. host_goodbye atomicity

## Missing Rationale / Examples

- [ ] 27. Wire walkthrough examples
- [ ] 28. Rationale gaps
