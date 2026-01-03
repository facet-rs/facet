+++
title = "Prioritization & QoS"
description = "Scheduling and quality of service"
weight = 80
+++

This document defines how Rapace handles request prioritization and quality of service (QoS). Priority enables important requests to be processed first under load.

## Priority Model

### Priority Levels

r[priority.value.range]
Priority values MUST be 8-bit unsigned integers (0-255). Higher values indicate higher priority.

| Range | Level | Use Case |
|-------|-------|----------|
| 0-31 | Background | Batch jobs, analytics, backups |
| 32-95 | Low | Non-critical operations, prefetching |
| 96-159 | Normal | Standard user requests (default: 128) |
| 160-223 | High | Interactive requests, real-time updates |
| 224-255 | Critical | Health checks, control plane, emergencies |

r[priority.value.default]
The default priority MUST be 128 (middle of Normal range) when no priority is specified.

### Priority Sources

Priority can be specified at multiple levels:

| Source | Scope | Precedence |
|--------|-------|------------|
| `rapace.priority` metadata | Per-call | Highest |
| `HIGH_PRIORITY` frame flag | Per-frame | Medium |
| Connection-level default | Per-connection | Lowest |

r[priority.precedence]
When multiple sources specify priority, implementations MUST apply them in this order (highest precedence first):
1. Per-call metadata overrides all
2. Frame flag sets priority to 192 if not otherwise specified
3. Connection default applies if nothing else specified (set via `rapace.default_priority` in Hello params; defaults to 128 if not set)

See [Handshake: Extension Parameters](@/spec/handshake.md#extension-parameters) for how to set connection-level default priority.

### Setting Priority

**Request metadata** (recommended):
```rust
OpenChannel {
    metadata: vec![
        ("rapace.priority".into(), vec![200]),  // High priority
    ],
    // ...
}
```

**Frame flag** (fast path for binary high/normal):
```rust
frame.flags |= FrameFlags::HIGH_PRIORITY;
```

r[priority.high-flag.mapping]
The `HIGH_PRIORITY` flag is a single-bit hint for schedulers that don't need fine-grained priority. When set, it MUST be interpreted as priority 192.

## Scheduling

### Priority Queue Dispatch

r[priority.scheduling.queue]
Servers SHOULD use priority-aware scheduling:

```rust
struct PriorityDispatcher {
    queues: [VecDeque<Request>; 8],  // 8 priority buckets
}

impl PriorityDispatcher {
    fn enqueue(&mut self, req: Request) {
        let bucket = (req.priority / 32) as usize;
        self.queues[bucket].push_back(req);
    }

    fn dequeue(&mut self) -> Option<Request> {
        // Highest priority first
        for queue in self.queues.iter_mut().rev() {
            if let Some(req) = queue.pop_front() {
                return Some(req);
            }
        }
        None
    }
}
```

### Weighted Fair Queuing

To prevent starvation of low-priority requests, use weighted fair queuing:

```rust
fn weighted_dequeue(&mut self) -> Option<Request> {
    // Probabilistically select bucket based on weights
    let weights = [1, 2, 4, 8, 16, 32, 64, 128];  // Exponential
    let total: u32 = weights.iter().sum();
    let mut pick = rand::random::<u32>() % total;

    for (i, &weight) in weights.iter().enumerate().rev() {
        if pick < weight && !self.queues[i].is_empty() {
            return self.queues[i].pop_front();
        }
        pick = pick.saturating_sub(weight);
    }

    // Fallback to strict priority
    self.dequeue()
}
```

Weight distribution:
- Priority 224-255 (Critical): 128 shares
- Priority 192-223 (High): 64 shares
- Priority 160-191 (High): 32 shares
- Priority 128-159 (Normal): 16 shares
- Priority 96-127 (Normal): 8 shares
- Priority 64-95 (Low): 4 shares
- Priority 32-63 (Low): 2 shares
- Priority 0-31 (Background): 1 share

This ensures low-priority requests still make progress, but high-priority requests get vastly more scheduling opportunities.

## Credit Allocation

### Priority-Based Credit Grants

Flow control credits can be allocated based on priority:

```rust
fn grant_credits(&mut self, channel: &Channel) -> u32 {
    let base_credits = 65536;
    let priority_factor = channel.priority as f64 / 128.0;
    
    // Higher priority = more credits
    (base_credits as f64 * priority_factor) as u32
}
```

### Credit Starvation Prevention

r[priority.credits.minimum]
Low-priority channels MUST receive minimum credits to prevent deadlock:

```rust
const MIN_CREDITS: u32 = 4096;  // Always grant at least 4KB

fn grant_credits(&mut self, channel: &Channel) -> u32 {
    let calculated = self.calculate_credits(channel);
    calculated.max(MIN_CREDITS)
}
```

### Priority Inversion

Priority inversion occurs when a high-priority request depends on a low-priority operation. To mitigate:

1. **Inherit priority**: When request A waits on channel B, temporarily boost B's priority to A's level
2. **Ceiling protocol**: Channels lock resources at highest priority that might use them
3. **Avoid blocking**: Use async patterns to prevent blocking high-priority on low-priority

## Per-Channel vs Per-Call Priority

### Channel Priority

Set at `OpenChannel` time, applies to all frames on that channel:

```rust
OpenChannel {
    metadata: vec![("rapace.priority".into(), vec![200])],
    // ...
}
```

Use for:
- Streaming channels with consistent priority
- Bulk data transfers (low priority)
- Real-time event streams (high priority)

### Call Priority

Embedded in the call's `OpenChannel` or request metadata:

```rust
// Each call can have different priority
call1.priority = 100;  // Normal
call2.priority = 200;  // High
```

Use for:
- Mixed-priority RPC traffic on single connection
- Dynamic priority based on user tier, request type, etc.

### Priority Propagation

When making downstream calls:

```rust
fn forward_request(&self, req: &Request) -> Request {
    let mut downstream = Request::new();
    
    // Propagate or reduce priority
    downstream.priority = req.priority.saturating_sub(10);
    
    downstream
}
```

r[priority.propagation.rules]
For downstream calls:
- Implementations SHOULD propagate priority for synchronous call chains
- Implementations SHOULD consider reducing priority for fan-out (prevents priority amplification)
- Implementations MUST cap downstream priority at the original request's priority

## Deadline Integration

Priority and deadlines work together:

### Deadline-Aware Scheduling

Requests closer to deadline get priority boost:

```rust
fn effective_priority(&self, req: &Request) -> u8 {
    let base = req.priority;
    
    if let Some(deadline) = req.deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let urgency = if remaining < Duration::from_millis(100) {
            50  // Almost expired: big boost
        } else if remaining < Duration::from_secs(1) {
            20  // Getting close
        } else {
            0   // Plenty of time
        };
        
        base.saturating_add(urgency)
    } else {
        base
    }
}
```

### Deadline-Based Shedding

When overloaded, prefer requests that can still complete:

```rust
fn should_shed(&self, req: &Request) -> bool {
    if let Some(deadline) = req.deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let estimated_processing = self.estimate_processing_time(req);
        
        // Shed if we can't possibly finish in time
        remaining < estimated_processing
    } else {
        false
    }
}
```

## Quality of Service Classes

For simpler configuration, define QoS classes:

```rust
enum QosClass {
    BestEffort,     // Priority 32, no deadline
    Standard,       // Priority 128, 30s deadline
    Interactive,    // Priority 192, 5s deadline
    Realtime,       // Priority 240, 100ms deadline
}

impl QosClass {
    fn apply(&self, req: &mut Request) {
        match self {
            QosClass::BestEffort => {
                req.priority = 32;
                req.deadline = None;
            }
            QosClass::Standard => {
                req.priority = 128;
                req.deadline = Some(Instant::now() + Duration::from_secs(30));
            }
            QosClass::Interactive => {
                req.priority = 192;
                req.deadline = Some(Instant::now() + Duration::from_secs(5));
            }
            QosClass::Realtime => {
                req.priority = 240;
                req.deadline = Some(Instant::now() + Duration::from_millis(100));
            }
        }
    }
}
```

## Fairness Guarantees

### What Rapace Guarantees

r[priority.guarantee.starvation]
Implementations using weighted fair queuing MUST ensure every priority level eventually gets service (no starvation).

r[priority.guarantee.ordering]
Higher priority requests SHOULD be more likely to be scheduled first.

r[priority.guarantee.deadline]
Implementations MUST track all pending requests with deadlines until completion or expiration; deadline-aware scheduling SHOULD be used.

### What Rapace Does NOT Guarantee

r[priority.non-guarantee]
Implementations are NOT required to provide:
1. **Strict priority**: High priority need not always preempt low priority
2. **Latency bounds**: Priority is best-effort, not a latency SLA
3. **Cross-connection fairness**: Each connection MAY have independent scheduling

### Monitoring Fairness

Track these metrics:

| Metric | Description |
|--------|-------------|
| `request_latency_by_priority` | Histogram per priority bucket |
| `requests_shed_by_priority` | Counter per priority bucket |
| `priority_inversion_events` | Counter of detected inversions |
| `starvation_warnings` | Counter when low-priority queue ages |

## Implementation Recommendations

### Clients

1. Set appropriate priority for each call type
2. Use `Interactive` for user-facing requests
3. Use `BestEffort` for background sync
4. Respect `retry_after` hints on overload

### Servers

1. Implement at least 4-bucket priority queue
2. Use weighted fair queuing to prevent starvation
3. Shed low-priority requests first under load
4. Monitor priority distribution

### Load Balancers

1. Forward priority metadata to backends
2. Consider priority in routing decisions (send critical to less-loaded servers)
3. Implement connection-level priority for health checks

## Next Steps

- [Core Protocol](@/spec/core.md) – HIGH_PRIORITY flag definition
- [Overload & Draining](@/spec/overload.md) – Priority-based load shedding
- [Metadata Conventions](@/spec/metadata.md) – `rapace.priority` key
- [Cancellation & Deadlines](@/spec/cancellation.md) – Deadline integration
