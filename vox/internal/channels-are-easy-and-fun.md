# Channels are easy and fun

## `sum`

```rust
#[roam::service]
trait Adder {
    async fn sum(&self, numbers: Rx<i32>) -> i64;
}
```

Generated handler trait:

```rust
trait Adder {
    async fn sum(&self, call: impl Call<i64, Infallible>, numbers: Rx<i32>);
}
```

Generated client method:

```rust
impl AdderClient {
    async fn sum(&self, numbers: Rx<i32>) -> Result<ResponseParts<i64>, RoamError>;
}
```

The handler receives `Rx<i32>`. It calls `numbers.recv()` in a loop, accumulating
values, then returns the total.

The caller creates a channel pair:

```rust
let (tx, rx) = roam::channel::<i32>();
```

The caller keeps `tx` to send items. It passes `rx` to the call:

```rust
client.sum(rx).await
```

`Rx<i32>` is in arg position. Per the spec, the caller allocates the channel ID.
The framework binds `rx` so the handler can receive from it. The framework also
needs to bind `tx` so the caller can send into the same channel.

## `start_ingester` — Rx in return position

```rust
#[roam::service]
trait Pipeline {
    async fn start_ingester(&self) -> Rx<Job>;
}
```

Generated handler trait:

```rust
trait PipelineServer {
    async fn start_ingester(&self, call: impl Call<Rx<Job>, Infallible>);
}
```

Generated client method:

```rust
impl PipelineClient {
    async fn start_ingester(&self) -> Result<ResponseParts<Rx<Job>>, RoamError>;
}
```

The handler starts a background processor, creates a channel pair, keeps
`tx` to pull work from, and returns `rx` via `call.ok(rx)`.

The caller receives `Rx<Job>` in the response. `Rx` means "I receive" —
and in return position, it's the *caller* who holds it, so the caller
receives. The handler keeps the paired `Tx<Job>` and sends jobs through it.
Direction: handler→caller.

Contrast with `sum`: `Rx<i32>` in arg position — the *handler* holds it,
so the handler receives. Direction: caller→handler.

Same type, opposite direction. `Rx` always means "I receive" and `Tx`
always means "I send". Position determines who "I" is.

`Rx<Job>` is in return position. Per the spec, the **callee** allocates the
channel ID. Position affects both who allocates the ID and the data direction.

## `generate` — Tx in arg position

```rust
#[roam::service]
trait Generator {
    async fn generate(&self, count: u32, output: Tx<i32>);
}
```

The handler holds `Tx<i32>`. `Tx` = "I send". Handler sends items to caller.
Direction: handler→caller.

The caller creates `(tx, rx) = channel()`, passes `tx` to the call, keeps
`rx` to receive items.

## `open_log` — Tx in return position

```rust
#[roam::service]
trait Logger {
    async fn open_log(&self, name: String) -> Tx<LogEntry>;
}
```

The caller holds `Tx<LogEntry>`. `Tx` = "I send". Caller sends log entries
to handler. Direction: caller→handler.

The handler creates the channel pair, keeps `rx` to consume entries, returns
`tx` via `call.ok(tx)`.

## Summary

|          | arg position (handler holds) | return position (caller holds) |
|----------|------------------------------|--------------------------------|
| `Rx<T>`  | handler receives ←caller     | caller receives ←handler       |
| `Tx<T>`  | handler sends →caller        | caller sends →handler          |
