# Vixen Vox Channel Consumers

Date: 2026-06-17.

Scope: read-only audit of `/Users/amos/vixenware/vixen` while that checkout was
dirty.

## Verdict

Vixen's important streams and remote trees should remain service-level demand
protocols, not raw Vox channels. The current `Producing::force(PartKey) -> Part`
shape is a good consumer model for the new Vox semantics: one requested part per
in-flight request, with producer state owned by Vixen, not by a raw channel.

Raw Vox channels in Vixen are progress/event sidebands. They are compatible
only when the service keeps the request in flight until the channel is
closed/drained.

## Consumer Map

- `crates/vx-runtime/src/remote.rs`: `Producing::force(part) -> Part`.
  Classification: good. Important stream/tree data is addressed by `PartKey`
  and pulled over request/response. A blocked part keeps the `force` request in
  flight. It does not require a raw channel to survive a response.

- `crates/vx-exec-protocol/src/lib.rs`: `Observe::observe(request) -> Observed`.
  Classification: good. `Observed::Tree` and `Observed::ValueWithTree` name a
  producing tree; actual data crosses later through the `Producing` service.

- `crates/vx-services/src/test_runner.rs`: `TestRunner::run_tests(request,
  events: Tx<Event>) -> RunSummary`.
  Classification: compatible request sideband. `crates/vx-daemon/src/test_runner.rs`
  forwards all events, emits `RunFinished`, drops the event sink, awaits the
  forwarder, then returns `RunSummary`. That keeps the request open until the
  channel is done.

- `crates/vx-services/src/orchestrator.rs`: `eval_function_with_progress(request,
  progress: Tx<EvalProgressEvent>) -> EvalFunctionResult`.
  Classification: API/design consumer; implementation was not found in the
  current grep. It should follow the same rule as `TestRunner`: progress channel
  closes/drains before the response. If it wants a result before future
  progress, it must become a service-level progress handle.

- `Store`, `CacheIndex`, `Runner`, `Daemon`, `Vfs`, `Vfsd`, and `Observe`
  ordinary methods.
  Classification: unaffected by raw channel lifetime; ordinary
  request/response.

## Migration Rule

For every Vixen method containing `Tx<T>` or `Rx<T>`:

- If the stream is just progress/events for the active call, keep the method in
  flight until the channel is terminal, then return.
- If the stream is a durable or independently demanded value, replace the raw
  channel with a service-level handle/protocol like `Producing::force`.
