# rapace-spec-tester-macros

Proc macros for the rapace spec tester.

Provides the `#[conformance]` attribute macro for registering test cases with their associated spec rules.

## Usage

```rust
use rapace_spec_tester_macros::conformance;

#[conformance(name = "handshake.valid_hello_exchange", rules = "handshake.hello.initiator")]
async fn valid_hello_exchange(peer: &mut Peer) -> TestResult {
    // test implementation
}
```
