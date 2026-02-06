# roam-stream AFL harness

This harness targets `roam_stream::CobsFramed::recv` to catch hangs, decode panics,
and framing edge cases (including large valid payloads).

## Build

```bash
cargo afl build --manifest-path fuzz/roam-stream-afl/Cargo.toml --bin cobs_recv_inner
```

## Seed corpus

Generate large, structurally valid seeds:

```bash
cargo run --manifest-path fuzz/roam-stream-afl/Cargo.toml --bin generate_seeds
```

## Fuzz

```bash
mkdir -p fuzz/roam-stream-afl/in
printf '\x01\x00' > fuzz/roam-stream-afl/in/minimal-frame

cargo afl fuzz \
  -i fuzz/roam-stream-afl/in \
  -o fuzz/roam-stream-afl/out \
  -- \
  fuzz/roam-stream-afl/target/debug/cobs_recv_inner
```

Large-input mode (focus on >= 32 KiB):

```bash
cargo afl fuzz \
  -g 32768 \
  -G 1048576 \
  -i fuzz/roam-stream-afl/in \
  -o fuzz/roam-stream-afl/out \
  -- \
  fuzz/roam-stream-afl/target/debug/cobs_recv_inner
```
