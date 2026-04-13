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

// Deeper-nested variant of the same bug: the map's value is a struct whose
// field is an enum-variant-of-Box-of-struct-with-missing-required-field. The
// failure unwinds several frames before `cleanup_stored_frames_on_error`
// reaches the Map; at that point the pending map key has already been
// committed / moved and popping + dropping it again segfaults with a bogus
// backing pointer (observed: String data_ptr = 0x4).
//
// Reduced from a facet-styx schema repro involving
//   Option<IndexMap<Documented<String>, TemplateDecl>> where
//   TemplateDecl.syntax = SyntaxExpr::Template(Box<TemplateSyntaxDecl>).
#[test]
fn deferred_map_value_deep_box_enum_missing_field() {
    #[derive(Facet, Debug)]
    struct InnerWithRequired {
        #[allow(dead_code)]
        required: String,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SyntaxExpr {
        Template(Box<InnerWithRequired>),
    }

    #[derive(Facet, Debug)]
    struct TemplateDecl {
        #[allow(dead_code)]
        syntax: SyntaxExpr,
    }

    #[derive(Facet, Debug)]
    struct Container {
        #[allow(dead_code)]
        templates: HashMap<String, TemplateDecl>,
    }

    let mut partial = Partial::alloc::<Container>().unwrap();
    partial = partial.begin_deferred().unwrap();

    partial = partial.begin_field("templates").unwrap();
    partial = partial.init_map().unwrap();

    partial = partial.begin_key().unwrap();
    partial = partial.set(String::from("ZeroOperand")).unwrap();
    partial = partial.end().unwrap();

    partial = partial.begin_value().unwrap(); // TemplateDecl
    partial = partial.begin_field("syntax").unwrap(); // SyntaxExpr
    partial = partial.select_variant_named("Template").unwrap();
    partial = partial.begin_nth_field(0).unwrap(); // Box<InnerWithRequired>
    partial = partial.begin_smart_ptr().unwrap(); // into the Box
    // Intentionally leave `required` unset.
    partial = partial.end().unwrap(); // end smart ptr
    partial = partial.end().unwrap(); // end tuple-variant field
    partial = partial.end().unwrap(); // end variant / syntax field
    partial = partial.end().unwrap(); // end value (TemplateDecl)
    partial = partial.end().unwrap(); // end templates field

    let result = partial.finish_deferred();
    assert!(
        result.is_err(),
        "expected Err from missing required field, got Ok"
    );
}
