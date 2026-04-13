//! Deferred-mode double-free repro: a stored MapValue frame whose value
//! struct has a missing required field triggers `cleanup_stored_frames_on_error`
//! during `finish_deferred()`. The map already added the pending entry
//! pointing at the value memory, so cleaning up the MapValue frame AND the
//! Map frame both drop the same data → double-free under ASAN.
//!
//! Minimal reduction from a facet-styx flatten-map repro that hit the same
//! codepath via the parser's deferred mode.

use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn deferred_map_value_missing_required_field() {
    #[derive(Facet, Debug)]
    struct RuleValue {
        required: String,
    }

    #[derive(Facet, Debug)]
    struct Container {
        rules: HashMap<String, RuleValue>,
    }

    let mut partial = Partial::alloc::<Container>().unwrap();
    partial = partial.begin_deferred().unwrap();

    partial = partial.begin_field("rules").unwrap();
    partial = partial.init_map().unwrap();
    partial = partial.begin_key().unwrap();
    partial = partial.set(String::from("ZeroOperand")).unwrap();
    partial = partial.end().unwrap();

    partial = partial.begin_value().unwrap();
    // Intentionally do NOT set `required` — leave RuleValue partially initialized.
    partial = partial.end().unwrap(); // end value
    partial = partial.end().unwrap(); // end map/field

    // finish_deferred should return Err (missing required field) without UB.
    let result = partial.finish_deferred();
    assert!(
        result.is_err(),
        "expected Err from missing required field, got Ok"
    );
}
