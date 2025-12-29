use std::collections::HashMap;

fn run_u64(label: &str, json: &str) {
    eprintln!("=== {label} ===");
    let shape = <HashMap<String, u64> as facet::Facet>::SHAPE;
    eprintln!("Shape: {:?}", shape.def);
    eprintln!(
        "Tier-1 compatible: {}",
        facet_format::jit::is_jit_compatible::<HashMap<String, u64>>()
    );
    eprintln!(
        "Tier-2 compatible: {}",
        facet_format::jit::is_format_jit_compatible::<HashMap<String, u64>>()
    );

    facet_format::jit::reset_tier_stats();
    let result = facet_format::jit::deserialize_with_format_jit_fallback::<HashMap<String, u64>, _>(
        facet_json::JsonParser::new(json.as_bytes()),
    );
    eprintln!("Result: {:?}", result);

    let (
        t2_attempts,
        t2_successes,
        t2_compile_unsup,
        t2_runtime_unsup,
        t2_runtime_err,
        t1_fallbacks,
    ) = facet_format::jit::get_tier_stats();
    eprintln!(
        "Stats: attempts={} successes={} compile_unsup={} runtime_unsup={} runtime_err={} t1_fallbacks={}",
        t2_attempts, t2_successes, t2_compile_unsup, t2_runtime_unsup, t2_runtime_err, t1_fallbacks
    );
}

fn run_string(label: &str, json: &str) {
    eprintln!("=== {label} ===");
    let shape = <HashMap<String, String> as facet::Facet>::SHAPE;
    eprintln!("Shape: {:?}", shape.def);
    eprintln!(
        "Tier-1 compatible: {}",
        facet_format::jit::is_jit_compatible::<HashMap<String, String>>()
    );
    eprintln!(
        "Tier-2 compatible: {}",
        facet_format::jit::is_format_jit_compatible::<HashMap<String, String>>()
    );

    facet_format::jit::reset_tier_stats();
    let result = facet_format::jit::deserialize_with_format_jit_fallback::<
        HashMap<String, String>,
        _,
    >(facet_json::JsonParser::new(json.as_bytes()));
    eprintln!("Result: {:?}", result);

    let (
        t2_attempts,
        t2_successes,
        t2_compile_unsup,
        t2_runtime_unsup,
        t2_runtime_err,
        t1_fallbacks,
    ) = facet_format::jit::get_tier_stats();
    eprintln!(
        "Stats: attempts={} successes={} compile_unsup={} runtime_unsup={} runtime_err={} t1_fallbacks={}",
        t2_attempts, t2_successes, t2_compile_unsup, t2_runtime_unsup, t2_runtime_err, t1_fallbacks
    );
}

fn main() {
    run_u64("simple_u64", r#"{"foo": 42, "bar": 123, "baz": 456}"#);

    // Exercises the owned-string path (escapes) for both keys and values.
    run_string(
        "escaped_key_and_value",
        r#"{"k\u0065y": "line\nfeed\tand\u2764"}"#,
    );
}
