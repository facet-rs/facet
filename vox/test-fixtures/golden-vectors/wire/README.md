# Wire Fixtures

These fixtures are generated from the canonical Rust wire model in
`rust/vox-types/src/message.rs` (nested `MessagePayload` and nested request/channel bodies).

Regenerate:

```bash
cargo run -p vox-core --bin generate_golden_vectors
```

These fixtures are for postcard-level golden coverage only. Cross-language
interoperability is covered by spec tests.
