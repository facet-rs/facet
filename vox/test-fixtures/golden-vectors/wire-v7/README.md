# Wire v7 Fixtures

These fixtures are generated from the canonical Rust v7 wire model in
`rust/roam-types/src/message.rs` (nested `MessagePayload` and nested request/channel bodies).

Regenerate:

```bash
cargo run -p roam-types --bin swift_wire_v7_fixtures
```

Current Swift runtime still uses the legacy flat wire message enum, so these
fixtures are intended for cross-language parity tracking during the wire v7 migration.
