+++
title = "Debugging and Observability"
weight = 40
+++

# Debugging and Observability

Picante provides comprehensive debugging and observability tools to help you understand and optimize your incremental computation system. This guide covers the available debugging features and how to use them.

## Overview

The `picante::debug` module provides:

- **Dependency graph visualization**: Export dependency graphs as Graphviz DOT format
- **Query execution tracing**: Record detailed traces of query execution with timing
- **Cache statistics**: Track cache usage, hits, and memory
- **Enhanced cycle diagnostics**: Clear error messages showing the full dependency cycle path

## Dependency Graph Visualization

Understanding the dependency relationships in your system is crucial for debugging unexpected recomputation and optimizing performance.

### Exporting Dependency Graphs

```rust
use picante::debug::DependencyGraph;

// Capture the current dependency graph
let graph = DependencyGraph::from_runtime(db.runtime());

// Export to Graphviz DOT format
graph.write_dot("deps.dot")?;
```

### Visualizing with Graphviz

Once you have the DOT file, you can visualize it using Graphviz tools:

```bash
# Generate PNG image
dot -Tpng deps.dot -o deps.png

# Generate SVG (better for large graphs)
dot -Tsvg deps.dot -o deps.svg

# Interactive visualization
xdot deps.dot
```

### Graph Analysis

The `DependencyGraph` provides methods for programmatic analysis:

```rust
// Find queries with no dependencies (roots)
let roots = graph.root_queries();

// Find queries that nothing depends on (leaves)
let leaves = graph.leaf_queries();

// Find all paths between two queries
let paths = graph.find_paths(&start_query, &end_query);
```

**Node Format**: Nodes are labeled as `kind_{id}_key_{hash}` where:
- `kind_{id}`: The query kind ID
- `key_{hash}`: 16-character hex hash of the key

## Query Execution Tracing

Trace events provide detailed insight into what queries are executing and why.

### Recording Traces

```rust
use picante::debug::TraceCollector;

// Start collecting trace events
let collector = TraceCollector::start(db.runtime());

// ... perform queries ...

// Stop collecting and get the trace
let trace = collector.stop().await;
println!("Recorded {} events", trace.len());
```

### Analyzing Traces

The `TraceAnalysis` type provides statistics about recorded events:

```rust
use picante::debug::TraceAnalysis;

let analysis = TraceAnalysis::from_trace(&trace);

println!("{}", analysis.format());
// Output:
// Trace Analysis:
//   Total events: 42
//   Input changes: 3
//   Invalidations: 12
//   Recomputations: 8
//   Duration: 145ms
//
//   Events by revision:
//     r1: 15 events
//     r2: 27 events
```

### Trace Event Types

The trace records these event types:

- **`RevisionBumped`**: The revision counter was incremented
- **`InputSet`**: An input value was set
- **`InputRemoved`**: An input value was removed
- **`QueryInvalidated`**: A derived query was invalidated due to a dependency change
- **`QueryChanged`**: A derived query recomputed and its output changed

Each event includes:
- Timestamp (for timing analysis)
- Revision number
- Query kind ID
- Key hash (for correlation)

### Continuous Monitoring

You can take snapshots without stopping the collector:

```rust
let collector = TraceCollector::start(db.runtime());

// ... some work ...
let snapshot1 = collector.snapshot().await;
println!("Events so far: {}", snapshot1.len());

// ... more work ...
let snapshot2 = collector.snapshot().await;
println!("Events so far: {}", snapshot2.len());

// Final trace
let final_trace = collector.stop().await;
```

## Cache Statistics

Understanding cache behavior helps optimize memory usage and performance.

### Collecting Statistics

```rust
use picante::debug::CacheStats;

let stats = CacheStats::collect(db.runtime());

println!("{}", stats.format());
// Output:
// Cache Statistics:
//   Forward deps: 156
//   Reverse deps: 89
//   Total edges: 342
//   Root queries: 12
//
//   Dependency count distribution:
//     0 deps: 12 queries
//     1 deps: 45 queries
//     2 deps: 67 queries
//     3 deps: 32 queries
```

### Statistics Fields

- **`forward_deps_count`**: Number of queries with recorded dependencies
- **`reverse_deps_count`**: Number of queries that have dependents
- **`total_dependency_edges`**: Total number of dependency relationships
- **`root_query_count`**: Queries with no dependencies (inputs or computed without deps)
- **`dep_count_histogram`**: Distribution showing how many queries have N dependencies

### Interpreting the Stats

**High dependency counts** may indicate:
- Queries that depend on many inputs (normal for aggregations)
- Opportunities to break queries into smaller pieces

**Many root queries** may indicate:
- Lots of independent inputs (normal)
- Queries that should share dependencies but don't

**Unbalanced distributions** may indicate:
- Some "hub" queries that many others depend on
- Opportunities for caching at different granularities

## Enhanced Cycle Detection

When a dependency cycle is detected, Picante now provides a clear path showing exactly how the cycle forms:

```rust
// This will produce a clear cycle error:
// dependency cycle detected
//   → kind_1, key_0000000000000001  (initial)
//   → kind_2, key_0000000000000002
//   → kind_3, key_0000000000000003
//   → kind_1, key_0000000000000001  ← cycle (already in stack)
```

The error shows:
1. The initial query in the cycle
2. All intermediate dependencies
3. The query that creates the cycle (attempting to depend on something already in the call stack)

## Best Practices

### Development Workflow

1. **Start with dependency graphs**: Visualize your system to understand the structure
2. **Use trace analysis for debugging**: When queries recompute unexpectedly, record a trace to see why
3. **Monitor cache stats**: Periodically check if your dependency structure is what you expect
4. **Profile with traces**: Use timestamps in trace events to identify slow queries

### Performance Debugging

When facing performance issues:

1. **Check the dependency graph**: Look for unexpected dependencies or cycles
2. **Analyze trace events**: Count invalidations and recomputations per revision
3. **Look at cache statistics**: Verify your queries are sharing dependencies as expected
4. **Profile with timing**: Use trace event timestamps to identify bottlenecks

### Production Monitoring

For production systems:

1. **Use `TraceCollector.snapshot()`**: Take periodic snapshots without stopping
2. **Aggregate statistics**: Track trends in `CacheStats` over time
3. **Alert on anomalies**: Sudden increases in invalidations or recomputations may indicate issues
4. **Export graphs periodically**: Visualize dependency evolution over time

## Example: Debugging Unexpected Recomputation

Here's a complete workflow for investigating why a query recomputes more than expected:

```rust
use picante::debug::{TraceCollector, TraceAnalysis, DependencyGraph};

// 1. Start tracing
let collector = TraceCollector::start(db.runtime());

// 2. Perform the operations that trigger unexpected recomputation
input.set(&db, key, value1);
let result1 = expensive_query.get(&db, key).await?;

input.set(&db, key, value2);  // Expect recomputation here
let result2 = expensive_query.get(&db, key).await?;

// 3. Analyze the trace
let trace = collector.stop().await;
let analysis = TraceAnalysis::from_trace(&trace);

println!("Recomputations: {}", analysis.recomputations);
println!("Invalidations: {}", analysis.invalidations);

// 4. Check the dependency graph
let graph = DependencyGraph::from_runtime(db.runtime());
graph.write_dot("debug.dot")?;

// 5. Find what expensive_query depends on
if let Some(deps) = graph.forward_deps.get(&expensive_query_key) {
    println!("Dependencies:");
    for dep in deps {
        println!("  - kind_{}, key_{:x}", dep.kind.0, dep.key.hash());
    }
}
```

## Integration with `tracing`

Picante already uses the [`tracing`](https://docs.rs/tracing) crate internally for detailed instrumentation. You can combine the debug tools with tracing subscribers for even more detailed analysis:

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Set up tracing to see detailed internal logs
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .init();

// Now Picante's internal tracing will be visible
// Set RUST_LOG=picante=trace to see all details
```

See the [tracing internals documentation](@/internals/tracing.md) for more information about Picante's tracing instrumentation.

## Summary

The debug tools provide multiple complementary views into your incremental system:

- **Dependency graphs**: Structural view of query relationships
- **Traces**: Temporal view of events and recomputation
- **Statistics**: Aggregate view of system state
- **Cycle diagnostics**: Clear errors when things go wrong

Use these tools together to understand, debug, and optimize your Picante applications.
