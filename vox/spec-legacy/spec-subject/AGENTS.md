<INSTRUCTIONS>
This is the “subject under test” binary used by the conformance suite.

Current state:
- It currently wraps the **legacy** Rust implementation (`rapace-core` / StreamTransport) to drive legacy conformance tests.

Rules:
- Avoid mixing new-suite work into this binary; create a separate subject binary under `spec/` for the new implementation.
</INSTRUCTIONS>
