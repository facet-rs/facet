# vox AFL Fuzzing

`cargo-afl` harnesses for Rust Vox protocol/state-machine behavior.

## Targets

- `protocol_decode`
  - Feeds arbitrary bytes into Vox postcard decode for `vox_types::Message`.
  - Re-encodes successfully decoded messages.
- `testbed_mem_session`
  - Runs generated `spec-proto` Testbed RPC traffic over in-memory initiator/acceptor+driver.
  - Exercises unary + streaming calls (`sum`, `generate`, `transform`) with fuzz-derived inputs.

## Build

```bash
cargo afl build --manifest-path fuzz/vox-afl/Cargo.toml --bin protocol_decode
cargo afl build --manifest-path fuzz/vox-afl/Cargo.toml --bin testbed_mem_session
```

## Run

```bash
cargo afl fuzz \
  -i fuzz/vox-afl/in/protocol_decode \
  -o fuzz/vox-afl/out/protocol_decode \
  -- fuzz/vox-afl/target/debug/protocol_decode

cargo afl fuzz \
  -i fuzz/vox-afl/in/testbed_mem_session \
  -o fuzz/vox-afl/out/testbed_mem_session \
  -- fuzz/vox-afl/target/debug/testbed_mem_session
```

For smoke runs:

```bash
timeout 60 cargo afl fuzz ...
```
