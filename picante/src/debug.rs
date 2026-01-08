//! Debugging and observability tools for Picante incremental computation.
//!
//! This module provides utilities to help understand and debug incremental query behavior:
//!
//! - **Dependency graph visualization**: Export the dependency graph as Graphviz DOT format
//! - **Query execution tracing**: Record detailed traces of query execution with timing
//! - **Cache statistics**: Track cache hits, misses, and memory usage
//! - **Cycle diagnostics**: Enhanced error messages showing the full dependency cycle path
//!
//! # Example
//!
//! ```no_run
//! use picante::debug::{DependencyGraph, CacheStats};
//! # use picante::{Runtime, HasRuntime};
//! # struct Db { runtime: Runtime }
//! # impl HasRuntime for Db { fn runtime(&self) -> &Runtime { &self.runtime } }
//! # fn main() -> std::io::Result<()> {
//! # let db = Db { runtime: Runtime::new() };
//!
//! // Export dependency graph
//! let graph = DependencyGraph::from_runtime(db.runtime());
//! graph.write_dot("deps.dot")?;
//!
//! // Get cache statistics
//! let stats = CacheStats::collect(db.runtime());
//! println!("Forward deps: {}, Reverse deps: {}",
//!          stats.forward_deps_count, stats.reverse_deps_count);
//! # Ok(())
//! # }
//! ```

use crate::key::{Dep, DynKey, QueryKindId};
use crate::revision::Revision;
use crate::runtime::{Runtime, RuntimeEvent};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// r[debug.graph]
/// A snapshot of the dependency graph for visualization and analysis.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Forward dependencies: for each query, what does it depend on?
    pub forward_deps: HashMap<DynKey, Vec<Dep>>,
    /// Reverse dependencies: for each query, what depends on it?
    pub reverse_deps: HashMap<DynKey, Vec<DynKey>>,
}

impl DependencyGraph {
    /// Capture the current dependency graph from a runtime.
    ///
    /// This creates a snapshot of both forward and reverse dependencies.
    /// Note that the graph only includes queries that have been executed
    /// and had their dependencies recorded.
    pub fn from_runtime(runtime: &Runtime) -> Self {
        let forward_deps = runtime
            .deps_by_query_snapshot()
            .into_iter()
            .map(|(key, deps)| (key, deps.to_vec()))
            .collect();

        let reverse_deps = runtime
            .reverse_deps_snapshot()
            .into_iter()
            .map(|(key, dependents)| (key, dependents.into_iter().collect()))
            .collect();

        Self {
            forward_deps,
            reverse_deps,
        }
    }

    /// Write the dependency graph in Graphviz DOT format.
    ///
    /// The output can be visualized using Graphviz tools:
    /// ```bash
    /// dot -Tpng deps.dot -o deps.png
    /// ```
    ///
    /// Nodes are labeled with `kind_id:key_hash` for uniqueness.
    /// Edges point from queries to their dependencies.
    pub fn write_dot<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        self.write_dot_to(&mut file)
    }

    /// Write the dependency graph in DOT format to any writer.
    pub fn write_dot_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "digraph dependencies {{")?;
        writeln!(writer, "  rankdir=LR;")?;
        writeln!(writer, "  node [shape=box];")?;
        writeln!(writer)?;

        // Collect all nodes
        let mut all_nodes = HashSet::new();
        for (query, deps) in &self.forward_deps {
            all_nodes.insert(query.clone());
            for dep in deps {
                all_nodes.insert(DynKey {
                    kind: dep.kind,
                    key: dep.key.clone(),
                });
            }
        }

        // Write node declarations
        for node in &all_nodes {
            let node_id = format!("{}_{:x}", node.kind.0, node.key.hash());
            let label = format!("kind_{}\\nkey_{:x}", node.kind.0, node.key.hash());
            writeln!(writer, "  {} [label=\"{}\"];", node_id, label)?;
        }

        writeln!(writer)?;

        // Write edges (query -> dependency)
        for (query, deps) in &self.forward_deps {
            let query_id = format!("{}_{:x}", query.kind.0, query.key.hash());
            for dep in deps {
                let dep_id = format!("{}_{:x}", dep.kind.0, dep.key.hash());
                writeln!(writer, "  {} -> {};", query_id, dep_id)?;
            }
        }

        writeln!(writer, "}}")?;
        Ok(())
    }

    /// Get all queries that have no dependencies (root queries).
    pub fn root_queries(&self) -> Vec<DynKey> {
        self.forward_deps
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Get all queries that nothing depends on (leaf queries).
    pub fn leaf_queries(&self) -> Vec<DynKey> {
        let mut all_queries: HashSet<DynKey> = self.forward_deps.keys().cloned().collect();

        // Remove queries that have dependents
        for dependents in self.reverse_deps.values() {
            for dependent in dependents {
                all_queries.remove(dependent);
            }
        }

        all_queries.into_iter().collect()
    }

    /// Find all dependency paths from `start` to `end`.
    ///
    /// Returns a list of paths, where each path is a sequence of dependencies.
    /// Returns empty vec if no path exists.
    ///
    /// Paths are limited to a maximum depth of 1000 to prevent stack overflow.
    pub fn find_paths(&self, start: &DynKey, end: &DynKey) -> Vec<Vec<Dep>> {
        let mut paths = Vec::new();
        let mut current_path = Vec::new();
        let mut visited = HashSet::new();

        self.find_paths_recursive(start, end, &mut current_path, &mut visited, &mut paths, 0);
        paths
    }

    fn find_paths_recursive(
        &self,
        current: &DynKey,
        end: &DynKey,
        path: &mut Vec<Dep>,
        visited: &mut HashSet<DynKey>,
        results: &mut Vec<Vec<Dep>>,
        depth: usize,
    ) {
        // Prevent stack overflow by limiting maximum depth
        const MAX_DEPTH: usize = 1000;
        if depth >= MAX_DEPTH {
            return;
        }

        if current == end {
            results.push(path.clone());
            return;
        }

        if visited.contains(current) {
            return;
        }

        visited.insert(current.clone());

        if let Some(deps) = self.forward_deps.get(current) {
            for dep in deps {
                let dep_key = DynKey {
                    kind: dep.kind,
                    key: dep.key.clone(),
                };
                path.push(dep.clone());
                self.find_paths_recursive(&dep_key, end, path, visited, results, depth + 1);
                path.pop();
            }
        }

        visited.remove(current);
    }
}

// r[debug.cache-stats]
/// Statistics about cache usage and performance.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of queries with recorded forward dependencies.
    pub forward_deps_count: usize,
    /// Number of queries with recorded reverse dependencies.
    pub reverse_deps_count: usize,
    /// Total number of dependency edges.
    pub total_dependency_edges: usize,
    /// Number of queries with no dependencies (roots).
    pub root_query_count: usize,
    /// Distribution of dependency counts.
    pub dep_count_histogram: HashMap<usize, usize>,
}

impl CacheStats {
    /// Collect cache statistics from the runtime.
    pub fn collect(runtime: &Runtime) -> Self {
        let forward = runtime.deps_by_query_snapshot();
        let reverse = runtime.reverse_deps_snapshot();

        let forward_deps_count = forward.len();
        let reverse_deps_count = reverse.len();

        let total_dependency_edges: usize = forward.values().map(|deps| deps.len()).sum();

        let root_query_count = forward.values().filter(|deps| deps.is_empty()).count();

        let mut dep_count_histogram = HashMap::new();
        for deps in forward.values() {
            *dep_count_histogram.entry(deps.len()).or_insert(0) += 1;
        }

        Self {
            forward_deps_count,
            reverse_deps_count,
            total_dependency_edges,
            root_query_count,
            dep_count_histogram,
        }
    }

    /// Format statistics as a human-readable string.
    pub fn format(&self) -> String {
        let mut s = String::new();
        s.push_str("Cache Statistics:\n");
        s.push_str(&format!("  Forward deps: {}\n", self.forward_deps_count));
        s.push_str(&format!("  Reverse deps: {}\n", self.reverse_deps_count));
        s.push_str(&format!("  Total edges: {}\n", self.total_dependency_edges));
        s.push_str(&format!("  Root queries: {}\n", self.root_query_count));

        if !self.dep_count_histogram.is_empty() {
            s.push_str("\n  Dependency count distribution:\n");
            let mut counts: Vec<_> = self.dep_count_histogram.iter().collect();
            counts.sort_by_key(|(count, _)| *count);
            for (count, queries) in counts {
                s.push_str(&format!("    {} deps: {} queries\n", count, queries));
            }
        }

        s
    }
}

/// A recorded trace event from query execution.
#[derive(Debug, Clone)]
pub enum TraceEvent {
    /// A revision was bumped.
    RevisionBumped {
        /// The new revision.
        revision: Revision,
        /// When this occurred.
        timestamp: Instant,
    },
    /// An input was set.
    InputSet {
        /// Revision when set.
        revision: Revision,
        /// Query kind.
        kind: QueryKindId,
        /// Key hash.
        key_hash: u64,
        /// When this occurred.
        timestamp: Instant,
    },
    /// An input was removed.
    InputRemoved {
        /// Revision when removed.
        revision: Revision,
        /// Query kind.
        kind: QueryKindId,
        /// Key hash.
        key_hash: u64,
        /// When this occurred.
        timestamp: Instant,
    },
    /// A query was invalidated.
    QueryInvalidated {
        /// Revision when invalidated.
        revision: Revision,
        /// Query kind.
        kind: QueryKindId,
        /// Key hash.
        key_hash: u64,
        /// Invalidated by this kind.
        by_kind: QueryKindId,
        /// Invalidated by this key hash.
        by_key_hash: u64,
        /// When this occurred.
        timestamp: Instant,
    },
    /// A query output changed.
    QueryChanged {
        /// Revision when changed.
        revision: Revision,
        /// Query kind.
        kind: QueryKindId,
        /// Key hash.
        key_hash: u64,
        /// When this occurred.
        timestamp: Instant,
    },
}

// r[debug.trace-collector]
/// A collector that records runtime events for analysis.
///
/// This subscribes to the runtime's event stream and records
/// all events with timestamps for later analysis.
///
/// # Example
///
/// ```no_run
/// use picante::debug::TraceCollector;
/// # use picante::Runtime;
/// # async fn example() {
/// let runtime = Runtime::new();
/// let collector = TraceCollector::start(&runtime);
///
/// // ... perform queries ...
///
/// let trace = collector.stop().await;
/// println!("Recorded {} events", trace.len());
/// # }
/// ```
pub struct TraceCollector {
    events: Arc<Mutex<Vec<TraceEvent>>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl TraceCollector {
    /// Start collecting trace events from the runtime.
    ///
    /// This spawns a background task that subscribes to runtime events
    /// and records them with timestamps.
    pub fn start(runtime: &Runtime) -> Self {
        let mut rx = runtime.subscribe_events();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let handle = tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                let timestamp = Instant::now();
                let trace_event = match event {
                    RuntimeEvent::RevisionBumped { revision } => TraceEvent::RevisionBumped {
                        revision,
                        timestamp,
                    },
                    RuntimeEvent::RevisionSet { .. } => {
                        // Skip RevisionSet as it's primarily for cache loading
                        continue;
                    }
                    RuntimeEvent::InputSet {
                        revision,
                        kind,
                        key_hash,
                        ..
                    } => TraceEvent::InputSet {
                        revision,
                        kind,
                        key_hash,
                        timestamp,
                    },
                    RuntimeEvent::InputRemoved {
                        revision,
                        kind,
                        key_hash,
                        ..
                    } => TraceEvent::InputRemoved {
                        revision,
                        kind,
                        key_hash,
                        timestamp,
                    },
                    RuntimeEvent::QueryInvalidated {
                        revision,
                        kind,
                        key_hash,
                        by_kind,
                        by_key_hash,
                        ..
                    } => TraceEvent::QueryInvalidated {
                        revision,
                        kind,
                        key_hash,
                        by_kind,
                        by_key_hash,
                        timestamp,
                    },
                    RuntimeEvent::QueryChanged {
                        revision,
                        kind,
                        key_hash,
                        ..
                    } => TraceEvent::QueryChanged {
                        revision,
                        kind,
                        key_hash,
                        timestamp,
                    },
                };

                events_clone.lock().await.push(trace_event);
            }
        });

        Self {
            events,
            _handle: handle,
        }
    }

    /// Stop collecting and return the recorded trace.
    ///
    /// This drains the collected events and returns them.
    /// The collector should not be used after calling stop.
    ///
    /// Note: This function gives a brief window for in-flight events to be
    /// processed before returning. However, it does not guarantee all events
    /// are captured if the runtime is still actively generating events.
    pub async fn stop(self) -> Vec<TraceEvent> {
        // Give a small amount of time for any in-flight events to be processed
        // This is best-effort - the background task will continue until the
        // Runtime's event channel is closed.
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Extract events
        let events = self.events.lock().await;
        events.clone()

        // We drop the handle, which allows the background task to continue
        // running but we've already extracted the events we care about.
    }

    /// Get a snapshot of currently collected events without stopping.
    pub async fn snapshot(&self) -> Vec<TraceEvent> {
        self.events.lock().await.clone()
    }
}

// r[debug.trace-analysis]
/// Analysis of a collected trace.
#[derive(Debug, Clone)]
pub struct TraceAnalysis {
    /// Total number of events.
    pub total_events: usize,
    /// Number of input changes.
    pub input_changes: usize,
    /// Number of query invalidations.
    pub invalidations: usize,
    /// Number of query recomputations (changes).
    pub recomputations: usize,
    /// Duration from first to last event.
    pub duration: Duration,
    /// Events grouped by revision.
    pub events_by_revision: HashMap<Revision, usize>,
}

impl TraceAnalysis {
    /// Analyze a collected trace.
    pub fn from_trace(trace: &[TraceEvent]) -> Self {
        if trace.is_empty() {
            return Self {
                total_events: 0,
                input_changes: 0,
                invalidations: 0,
                recomputations: 0,
                duration: Duration::ZERO,
                events_by_revision: HashMap::new(),
            };
        }

        let mut input_changes = 0;
        let mut invalidations = 0;
        let mut recomputations = 0;
        let mut events_by_revision: HashMap<Revision, usize> = HashMap::new();

        let first_timestamp = match trace.first() {
            Some(TraceEvent::RevisionBumped { timestamp, .. })
            | Some(TraceEvent::InputSet { timestamp, .. })
            | Some(TraceEvent::InputRemoved { timestamp, .. })
            | Some(TraceEvent::QueryInvalidated { timestamp, .. })
            | Some(TraceEvent::QueryChanged { timestamp, .. }) => *timestamp,
            None => Instant::now(),
        };

        let last_timestamp = match trace.last() {
            Some(TraceEvent::RevisionBumped { timestamp, .. })
            | Some(TraceEvent::InputSet { timestamp, .. })
            | Some(TraceEvent::InputRemoved { timestamp, .. })
            | Some(TraceEvent::QueryInvalidated { timestamp, .. })
            | Some(TraceEvent::QueryChanged { timestamp, .. }) => *timestamp,
            None => first_timestamp,
        };

        for event in trace {
            let revision = match event {
                TraceEvent::RevisionBumped { revision, .. }
                | TraceEvent::InputSet { revision, .. }
                | TraceEvent::InputRemoved { revision, .. }
                | TraceEvent::QueryInvalidated { revision, .. }
                | TraceEvent::QueryChanged { revision, .. } => *revision,
            };

            *events_by_revision.entry(revision).or_insert(0) += 1;

            match event {
                TraceEvent::InputSet { .. } | TraceEvent::InputRemoved { .. } => {
                    input_changes += 1;
                }
                TraceEvent::QueryInvalidated { .. } => {
                    invalidations += 1;
                }
                TraceEvent::QueryChanged { .. } => {
                    recomputations += 1;
                }
                _ => {}
            }
        }

        Self {
            total_events: trace.len(),
            input_changes,
            invalidations,
            recomputations,
            duration: last_timestamp.duration_since(first_timestamp),
            events_by_revision,
        }
    }

    /// Format the analysis as a human-readable string.
    pub fn format(&self) -> String {
        let mut s = String::new();
        s.push_str("Trace Analysis:\n");
        s.push_str(&format!("  Total events: {}\n", self.total_events));
        s.push_str(&format!("  Input changes: {}\n", self.input_changes));
        s.push_str(&format!("  Invalidations: {}\n", self.invalidations));
        s.push_str(&format!("  Recomputations: {}\n", self.recomputations));
        s.push_str(&format!("  Duration: {:?}\n", self.duration));

        if !self.events_by_revision.is_empty() {
            s.push_str("\n  Events by revision:\n");
            let mut revisions: Vec<_> = self.events_by_revision.iter().collect();
            revisions.sort_by_key(|(rev, _)| rev.0);
            for (revision, count) in revisions {
                s.push_str(&format!("    r{}: {} events\n", revision.0, count));
            }
        }

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph_empty() {
        let runtime = Runtime::new();
        let graph = DependencyGraph::from_runtime(&runtime);

        assert_eq!(graph.forward_deps.len(), 0);
        assert_eq!(graph.reverse_deps.len(), 0);
    }

    #[test]
    fn test_cache_stats_empty() {
        let runtime = Runtime::new();
        let stats = CacheStats::collect(&runtime);

        assert_eq!(stats.forward_deps_count, 0);
        assert_eq!(stats.reverse_deps_count, 0);
        assert_eq!(stats.total_dependency_edges, 0);
        assert_eq!(stats.root_query_count, 0);
    }

    #[test]
    fn test_stats_format() {
        let runtime = Runtime::new();
        let stats = CacheStats::collect(&runtime);
        let formatted = stats.format();

        assert!(formatted.contains("Cache Statistics"));
        assert!(formatted.contains("Forward deps: 0"));
    }

    #[test]
    fn test_trace_analysis_empty() {
        let trace: Vec<TraceEvent> = vec![];
        let analysis = TraceAnalysis::from_trace(&trace);

        assert_eq!(analysis.total_events, 0);
        assert_eq!(analysis.input_changes, 0);
        assert_eq!(analysis.invalidations, 0);
        assert_eq!(analysis.recomputations, 0);
    }
}
