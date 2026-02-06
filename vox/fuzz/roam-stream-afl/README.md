# roam-stream AFL harness

This harness targets the low-level frame decoder (`try_decode_one_from_buffer_for_fuzz`)
used by `CobsFramed::recv`, so fuzzing stays focused on delimiter scanning and frame
decode behavior without async runtime overhead.

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
