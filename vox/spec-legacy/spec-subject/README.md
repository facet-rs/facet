# rapace-spec-subject

Real rapace-core implementation for spec conformance testing.

This binary uses the actual `rapace-core` RpcSession and StreamTransport to communicate with `rapace-spec-tester` via stdin/stdout.

## Usage

```bash
rapace-spec-subject --case handshake.valid_hello_exchange
```

The test orchestrator (`rapace-spec-tests`) spawns this alongside `rapace-spec-tester` and pipes them together.
