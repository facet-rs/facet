+++
title = "Deployment & Best Practices"
description = "Guidance for deploying and operating Rapace services"
weight = 50
+++

This document provides deployment recommendations and best practices for Rapace services. Unlike the [Specification](/spec/), the guidance here is **non-normative**—it represents recommended practices rather than protocol requirements.

## Observability

### Performance Overhead

Observability should not significantly impact performance. Recommendations:

- Basic metrics should be cheap to collect
- Detailed tracing may be sampled to reduce overhead
- Use OpenTelemetry semantic conventions where applicable

### Structured Logging

All logs should include these fields for correlation:

| Field | Description |
|-------|-------------|
| `trace_id` | Trace ID (if tracing enabled) |
| `span_id` | Span ID (if tracing enabled) |
| `connection_id` | Connection identifier |
| `channel_id` | Channel ID (if applicable) |
| `method_id` | Method ID (if applicable) |
| `service` | Service name (if applicable) |
| `method` | Method name (if applicable) |

### Sampling Decisions

Implementations may override sampling decisions locally based on:

- Error responses (always sample errors)
- High latency (sample slow requests)
- Debug mode (sample everything)

See [Observability Specification](/spec/observability/) for span naming conventions, metric definitions, and instrumentation patterns.

## Overload Handling

### Server-Side Metrics

Servers should monitor these indicators to detect overload:

| Indicator | Threshold | Recommended Action |
|-----------|-----------|-------------------|
| Pending requests | > max_pending_calls | Reject new calls |
| Memory usage | > 80% of limit | Start shedding |
| CPU utilization | > 90% sustained | Shed low-priority |
| Request latency | > 2x baseline | Shed non-critical |
| SHM slot exhaustion | 0 free slots | Block or reject |
| Channel count | > max_channels | Reject new channels |

### Load Shedding Order

When overloaded, servers should shed load in this priority order:

1. **Reject new connections**: Stop accepting transport connections
2. **Reject new calls**: Return `RESOURCE_EXHAUSTED` immediately
3. **Cancel low-priority requests**: Cancel requests with `priority < 96`
4. **Deadline-based shedding**: Cancel requests that can't finish in time

### Client Retry Behavior

When receiving `RESOURCE_EXHAUSTED` or `UNAVAILABLE`:

1. Check the `rapace.retryable` trailer; if `0`, do not retry
2. Wait at least `rapace.retry_after_ms` milliseconds if present
3. If no `retry_after` is provided, use exponential backoff with random jitter

**Backoff formula:**
```rust
fn backoff(attempt: u32, base_ms: u64, max_ms: u64) -> Duration {
    let backoff = base_ms * 2u64.pow(attempt.min(10));
    let capped = backoff.min(max_ms);
    let jitter = rand::random::<f64>() * 0.3;  // 0-30% jitter
    Duration::from_millis((capped as f64 * (1.0 + jitter)) as u64)
}
```

### Circuit Breakers

Clients should implement circuit breakers:

| State | Behavior |
|-------|----------|
| Closed | Normal operation |
| Open | Fail immediately, no RPC |
| Half-Open | Allow one probe RPC |

Transition rules:
- Closed → Open: N consecutive failures
- Open → Half-Open: After cooldown period
- Half-Open → Closed: Probe succeeds
- Half-Open → Open: Probe fails

### GoAway Client Behavior

When receiving `GoAway`:

1. Stop sending new calls on this connection
2. Allow pending in-flight calls to complete
3. Establish a new connection to the same or different server
4. Do not flood with retries; respect the drain window
5. Use exponential backoff if reconnecting to the same server
6. Load balance to different servers if available
7. Log the GoAway reason for debugging

## Security Profiles

Rapace does not mandate specific security mechanisms. Choose a security profile based on your deployment environment.

### Profile A: Trusted Local

**Environment**: Same process, same trust domain, localhost-only communication.

- Transport security is optional
- Multi-tenant deployments must still authenticate at the application layer
- Use Unix sockets with appropriate file permissions for IPC

**Examples**:
- In-process service mesh sidecar
- Same-host microservices under single operator
- Development/testing environments

### Profile B: Same Host, Untrusted

**Environment**: Same machine, but different trust domains (plugins, multi-tenant workloads).

- Authenticate peers at the RPC layer (token in Hello params or per-call metadata)
- Authorize each call based on the authenticated identity
- Use OS-level isolation (containers, namespaces, seccomp)
- Use SHM transport with appropriate permissions

**Examples**:
- Plugin system where plugins are untrusted
- Multi-tenant SaaS on shared infrastructure
- Sandboxed extensions

### Profile C: Networked / Untrusted

**Environment**: Communication over networks, potentially hostile environments.

- Use confidentiality and integrity protection (TLS 1.3+, QUIC, WireGuard)
- Authenticate peers (mutual TLS, bearer tokens)
- Reject connections with invalid or missing authentication
- Use certificate pinning for high-security deployments

**Examples**:
- Microservices across data centers
- Client-server applications
- Public-facing APIs

### Metadata Security

Hello params and OpenChannel metadata are **not encrypted** by Rapace—they are transmitted as plaintext in the Rapace payload.

- Do not put sensitive data (passwords, long-lived secrets) in metadata without transport encryption
- Tokens in metadata should be short-lived and scoped
- For sensitive operations, use transport-level security (TLS) as the foundation

### Deployment Matrix

| Deployment | Transport | Auth | Notes |
|------------|-----------|------|-------|
| In-process | Direct call | N/A | No Rapace needed |
| Same-host trusted | Unix socket / SHM | Optional | Use file permissions |
| Same-host untrusted | SHM + token | Required | Validate on every call |
| LAN (trusted) | TCP + TLS optional | Token or mTLS | Defense in depth |
| WAN / Internet | TCP + TLS required | mTLS or token | Always encrypt |
| Browser | WebSocket + TLS | Token | Use WSS only |

### Security Checklist

For production deployments:

- [ ] Identify trust profile (A, B, or C)
- [ ] Configure appropriate transport security
- [ ] Implement authentication in Hello params or per-call
- [ ] Implement authorization checks on service methods
- [ ] Set appropriate timeouts and rate limits
- [ ] Log authentication failures
- [ ] Rotate secrets regularly

## Kubernetes Integration

For graceful shutdown in Kubernetes:

```yaml
spec:
  terminationGracePeriodSeconds: 60
  containers:
  - name: myservice
    lifecycle:
      preStop:
        exec:
          command: ["/bin/sh", "-c", "kill -SIGTERM 1 && sleep 55"]
```

The application should:

1. Catch `SIGTERM`
2. Start draining (send GoAway)
3. Wait for connections to drain
4. Exit within `terminationGracePeriodSeconds`

## Next Steps

- [Observability Specification](/spec/observability/) – detailed span/metric conventions
- [Overload & Draining Specification](/spec/overload/) – protocol-level requirements
- [Security Profiles Specification](/spec/security/) – authentication failure handling
