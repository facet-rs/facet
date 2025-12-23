//! Integration tests for debugging and observability tools.

use picante::debug::{CacheStats, DependencyGraph, TraceAnalysis, TraceCollector};
use picante::{
    DerivedIngredient, HasRuntime, IngredientLookup, IngredientRegistry, InputIngredient,
    QueryKindId, Runtime,
};
use std::sync::Arc;

#[derive(Default)]
struct TestDb {
    runtime: Runtime,
    ingredients: IngredientRegistry<TestDb>,
}

impl HasRuntime for TestDb {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

impl IngredientLookup for TestDb {
    fn ingredient(&self, kind: QueryKindId) -> Option<&dyn picante::DynIngredient<Self>> {
        self.ingredients.ingredient(kind)
    }
}

#[tokio::test]
async fn test_dependency_graph_visualization() {
    let mut db = TestDb::default();

    // Create input ingredient
    let input: Arc<InputIngredient<u32, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "input"));

    // Create derived ingredient that depends on input
    let derived: Arc<DerivedIngredient<TestDb, u32, usize>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "derived",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len())
                })
            },
        ))
    };

    db.ingredients.register(input.clone());
    db.ingredients.register(derived.clone());

    // Set input and trigger derived computation
    input.set(&db, 1, "hello".to_string());
    input.set(&db, 2, "world".to_string());

    let _ = derived.get(&db, 1).await.unwrap();
    let _ = derived.get(&db, 2).await.unwrap();

    // Get dependency graph
    let graph = DependencyGraph::from_runtime(db.runtime());

    // Verify we have dependencies recorded
    assert!(!graph.forward_deps.is_empty(), "Should have forward deps");

    // Write DOT format to string
    let mut output = Vec::new();
    graph.write_dot_to(&mut output).unwrap();
    let dot = String::from_utf8(output).unwrap();

    assert!(dot.contains("digraph dependencies"));
    assert!(dot.contains("->"));
}

#[tokio::test]
async fn test_cache_statistics() {
    let mut db = TestDb::default();

    let input: Arc<InputIngredient<u32, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "input"));

    let derived: Arc<DerivedIngredient<TestDb, u32, usize>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "derived",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len())
                })
            },
        ))
    };

    db.ingredients.register(input.clone());
    db.ingredients.register(derived.clone());

    // Set some inputs and compute
    input.set(&db, 1, "hello".to_string());
    input.set(&db, 2, "world".to_string());
    input.set(&db, 3, "foo".to_string());

    let _ = derived.get(&db, 1).await.unwrap();
    let _ = derived.get(&db, 2).await.unwrap();
    let _ = derived.get(&db, 3).await.unwrap();

    // Collect stats
    let stats = CacheStats::collect(db.runtime());

    assert!(stats.forward_deps_count > 0);
    assert!(stats.total_dependency_edges > 0);

    // Format stats
    let formatted = stats.format();
    assert!(formatted.contains("Cache Statistics"));
    assert!(formatted.contains("Forward deps:"));
}

#[tokio::test]
async fn test_trace_collector() {
    let mut db = TestDb::default();

    let input: Arc<InputIngredient<u32, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "input"));

    let derived: Arc<DerivedIngredient<TestDb, u32, usize>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "derived",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len())
                })
            },
        ))
    };

    db.ingredients.register(input.clone());
    db.ingredients.register(derived.clone());

    // Start collecting trace
    let collector = TraceCollector::start(db.runtime());

    // Perform operations
    input.set(&db, 1, "hello".to_string());
    let _ = derived.get(&db, 1).await.unwrap();

    // Give time for events to be collected
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Stop and get trace
    let trace = collector.stop().await;

    // Verify we recorded events
    assert!(!trace.is_empty(), "Should have recorded events");

    // Analyze trace
    let analysis = TraceAnalysis::from_trace(&trace);
    assert!(analysis.total_events > 0);
    assert!(analysis.input_changes > 0);

    // Format analysis
    let formatted = analysis.format();
    assert!(formatted.contains("Trace Analysis"));
    assert!(formatted.contains("Total events:"));
}

#[tokio::test]
async fn test_trace_analysis_with_invalidations() {
    let mut db = TestDb::default();

    let input: Arc<InputIngredient<u32, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "input"));

    let derived: Arc<DerivedIngredient<TestDb, u32, usize>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "derived",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len())
                })
            },
        ))
    };

    db.ingredients.register(input.clone());
    db.ingredients.register(derived.clone());

    // Start collecting
    let collector = TraceCollector::start(db.runtime());

    // Initial computation
    input.set(&db, 1, "hello".to_string());
    let result1 = derived.get(&db, 1).await.unwrap();
    assert_eq!(result1, 5);

    // Change input (should invalidate derived)
    input.set(&db, 1, "world!".to_string());
    let result2 = derived.get(&db, 1).await.unwrap();
    assert_eq!(result2, 6);

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let trace = collector.stop().await;
    let analysis = TraceAnalysis::from_trace(&trace);

    // Should have input changes and invalidations
    assert!(
        analysis.input_changes >= 2,
        "Should have at least 2 input changes"
    );
    assert!(analysis.invalidations > 0, "Should have invalidations");
}

#[tokio::test]
async fn test_dependency_graph_path_finding() {
    let mut db = TestDb::default();

    let input: Arc<InputIngredient<u32, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "input"));

    let derived1: Arc<DerivedIngredient<TestDb, u32, usize>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "derived1",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len())
                })
            },
        ))
    };

    let derived2: Arc<DerivedIngredient<TestDb, u32, usize>> = {
        let derived1 = derived1.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(3),
            "derived2",
            move |db, key| {
                let derived1 = derived1.clone();
                Box::pin(async move {
                    let len = derived1.get(db, key).await?;
                    Ok(len * 2)
                })
            },
        ))
    };

    db.ingredients.register(input.clone());
    db.ingredients.register(derived1.clone());
    db.ingredients.register(derived2.clone());

    // Compute through the chain
    input.set(&db, 1, "hello".to_string());
    let _ = derived2.get(&db, 1).await.unwrap();

    let graph = DependencyGraph::from_runtime(db.runtime());

    // Verify we captured the dependency chain
    assert!(
        graph.forward_deps.len() >= 2,
        "Should have chain of dependencies"
    );
}
