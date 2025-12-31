# rapace-conformance

Conformance test suite for the Rapace protocol specification.

## Overview

This crate provides a reference peer that implementations (Rust, Swift, TypeScript, etc.)
can use to validate their conformance to the Rapace specification.

The reference peer communicates via stdin/stdout using length-prefixed Rapace frames,
making it easy to integrate with any language's test framework.

## Usage

### List available tests

```bash
rapace-conformance --list
rapace-conformance --list --show-rules    # Show spec rules covered
rapace-conformance --list --format json   # Machine-readable output
```

### Run a test

```bash
rapace-conformance --case handshake.valid_hello_exchange
```

The peer reads frames from stdin and writes frames to stdout.

### Exit codes

- `0`: Test passed
- `1`: Test failed (protocol violation detected)
- `2`: Internal error

## Wire format

Frames are length-prefixed:
- 4 bytes: total length (little-endian u32)
- 64 bytes: MsgDescHot descriptor
- N bytes: payload (if not inline)

## Test categories

- `handshake.*` - Hello exchange, version negotiation, features
- `frame.*` - Descriptor format, sentinels, encoding
- `channel.*` - Channel ID allocation, parity, lifecycle
- `call.*` - Request/response semantics, flags, errors
- `control.*` - Ping/pong, GoAway, unknown verbs
- `error.*` - Status codes, error handling

## Integration with Rust tests

The crate includes a libtest-mimic harness that runs structural tests as Rust unit tests:

```bash
cargo nextest run -p rapace-conformance
```
