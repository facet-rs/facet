# HTTP Tunnel Benchmark

This document describes the benchmarking setup for measuring rapace TCP tunnel overhead.

## Questions Answered

1. **What's the overhead of the rapace TCP tunnel versus direct HTTP?**
2. **How does overhead change with:**
   - Concurrency (1, 8, 64, 256)
   - Response size (small ~2 bytes vs large ~256KB)
   - Transport type (stream vs SHM)
3. **Do we see pathologies (throughput collapse, latency spikes) under load?**

## Benchmark Components

### Binaries

| Binary | Description |
|--------|-------------|
| `http_baseline` | Direct axum HTTP server (no rapace) |
| `http-tunnel-host` | Host side: accepts HTTP, tunnels through rapace |
| `http-tunnel-plugin` | Plugin side: receives tunnel data, forwards to internal HTTP |

### Endpoints

| Endpoint | Response Size | Purpose |
|----------|---------------|---------|
| `/small` | 2 bytes ("ok") | Measures per-request overhead |
| `/large` | ~256KB | Measures throughput/bandwidth overhead |
| `/health` | 2 bytes | Health check for startup detection |

### Transports

| Transport | Description |
|-----------|-------------|
| Stream (Unix Socket) | `rapace-transport-stream` over Unix domain socket |
| SHM | `rapace-transport-shm` shared memory transport |

## Running Benchmarks

### Prerequisites

1. Install oha: `cargo install oha`
2. Build binaries: `cargo build --release -p rapace-http-tunnel`

### Run

```bash
./scripts/bench_http_tunnel.sh
```

Results are saved to `bench_results/<timestamp>/`.

### Manual Testing

```bash
# Baseline
./target/release/http_baseline &
oha http://127.0.0.1:4000/small -z 10s -c 8

# Tunnel over Unix socket
./target/release/http-tunnel-host --transport=stream --addr=/tmp/rapace.sock &
./target/release/http-tunnel-plugin --transport=stream --addr=/tmp/rapace.sock &
oha http://127.0.0.1:4000/small -z 10s -c 8

# Tunnel over SHM
./target/release/http-tunnel-host --transport=shm --addr=/tmp/rapace.shm &
./target/release/http-tunnel-plugin --transport=shm --addr=/tmp/rapace.shm &
oha http://127.0.0.1:4000/small -z 10s -c 8
```

## Metrics Collected

From oha JSON output:

| Metric | Description |
|--------|-------------|
| `requestsPerSec` | Throughput (RPS) |
| `latencyPercentiles.p50` | Median latency |
| `latencyPercentiles.p90` | 90th percentile latency |
| `latencyPercentiles.p99` | 99th percentile latency |

## Results Template

After running benchmarks, results are formatted as:

### Small Response (2 bytes)

| Transport | Concurrency | RPS | p50 (ms) | p99 (ms) | RPS vs Baseline |
|-----------|-------------|-----|----------|----------|-----------------|
| Baseline  | 1 | - | - | - | 100% |
| Stream    | 1 | - | - | - | -% |
| SHM       | 1 | - | - | - | -% |
| ...       | ... | ... | ... | ... | ... |

### Large Response (~256KB)

| Transport | Concurrency | RPS | p50 (ms) | p99 (ms) | RPS vs Baseline |
|-----------|-------------|-----|----------|----------|-----------------|
| Baseline  | 1 | - | - | - | 100% |
| Stream    | 1 | - | - | - | -% |
| SHM       | 1 | - | - | - | -% |
| ...       | ... | ... | ... | ... | ... |

## Architecture

```
Browser (oha)
    │
    │ HTTP (port 4000)
    ▼
┌───────────────────┐
│ http-tunnel-host  │  (or http_baseline for baseline)
│ TunnelHost        │
└───────────────────┘
    │
    │ rapace tunnel (stream or SHM)
    ▼
┌───────────────────┐
│ http-tunnel-plugin│
│ TcpTunnelImpl     │
└───────────────────┘
    │
    │ TCP (port 9876)
    ▼
┌───────────────────┐
│ Internal axum     │
│ /small, /large    │
└───────────────────┘
```

## Zero-Copy Metrics (TODO)

For SHM transport, track:
- `tunnel_zero_copy_bytes`: Bytes transferred via SHM slot references
- `tunnel_copied_bytes`: Bytes that required copying

This helps verify SHM is actually avoiding copies for large payloads.

## Interpreting Results

### Good Results
- Stream overhead: 10-30% RPS drop vs baseline
- SHM overhead: 5-15% RPS drop vs baseline (should be better than stream)
- p99 latency: < 2x baseline at low concurrency

### Warning Signs
- RPS collapse at high concurrency (indicates contention)
- p99 latency > 10x baseline (indicates head-of-line blocking)
- SHM worse than stream (indicates SHM overhead, possible misconfiguration)
