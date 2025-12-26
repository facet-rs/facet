# rapace-tracing

[![crates.io](https://img.shields.io/crates/v/rapace-tracing.svg)](https://crates.io/crates/rapace-tracing)
[![documentation](https://docs.rs/rapace-tracing/badge.svg)](https://docs.rs/rapace-tracing)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-tracing.svg)](./LICENSE)

Tracing subscriber that forwards spans and events over rapace RPC.

This crate enables plugins to use `tracing` normally while having all spans and events collected in the host process via rapace RPC.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                             PLUGIN PROCESS                              │
│                                                                         │
│   tracing::info!("hello") ──► RapaceTracingLayer ──► TracingSinkClient ─┤
│                                      ▲                                  │
│                                      │                                  │
│                          TracingConfigServer ◄──────────────────────────┤
│                          (applies host's filter)                        │
└────────────────────────────────────────────────────────────────────────┬┘
                                                                         │
                             rapace transport (TCP/Unix/SHM)             │
                                                                         │
┌────────────────────────────────────────────────────────────────────────┴┐
│                              HOST PROCESS                               │
│                                                                         │
│   TracingSinkServer ──► HostTracingSink ──► tracing_subscriber / logs  │
│                                                                         │
│   TracingConfigClient ──► pushes filter changes to plugin              │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Filter Flow

The host is the single source of truth for log filtering:

1. Host decides what log levels/targets are enabled
2. Host pushes filter config to plugin via `TracingConfig::set_filter`
3. Plugin applies the filter locally (avoids spam over RPC)
4. When host changes filters dynamically, it pushes the update

## Example

```rust
// Plugin side: install the layer
let layer = RapaceTracingLayer::new(sink_client);
tracing_subscriber::registry().with(layer).init();

// Now all tracing calls are forwarded to the host
tracing::info!("hello from plugin");
```

## License

MIT OR Apache-2.0
