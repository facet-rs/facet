use std::collections::HashMap;
use std::sync::Once;

use facet::Facet;

use crate::{Documented, from_str};

fn init_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("facet_styx=trace".parse().unwrap())
                    .add_directive("facet_format=trace".parse().unwrap()),
            )
            .with_test_writer()
            .try_init();
    });
}

#[derive(Facet, Debug, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
enum ExprPayload {
    Scalar(String),
    Seq(Vec<Expr>),
    Object(ExprObject),
}

#[derive(Facet, Debug, PartialEq)]
struct ExprObject {
    #[facet(flatten)]
    fields: HashMap<Documented<String>, Expr>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "snake_case")]
#[repr(u8)]
enum Expr {
    Template(Box<TemplateDecl>),
    Regex(Vec<Expr>),
    #[facet(rename = "Any")]
    Any,
    #[facet(other)]
    Other {
        #[facet(tag)]
        tag: Option<String>,
        #[facet(content)]
        content: Option<ExprPayload>,
    },
}

#[derive(Facet, Debug, PartialEq)]
struct TemplateDecl {
    params: Vec<Expr>,
    body: Box<TemplateBody>,
}

#[derive(Facet, Debug, PartialEq)]
struct TemplateBody {
    syntax: Box<Expr>,
    highlight: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct Doc {
    v: Expr,
}

#[test]
fn mixed_expr_accepts_any_tag() {
    init_tracing();
    let parsed: Doc = from_str("v @Any").unwrap();
    assert_eq!(parsed.v, Expr::Any);
}

#[test]
fn mixed_expr_accepts_sequence_with_object_binding() {
    init_tracing();
    let parsed: Doc = from_str("v ({text @Any})").unwrap();
    match parsed.v {
        Expr::Other {
            tag: None,
            content: Some(ExprPayload::Seq(items)),
        } => assert_eq!(items.len(), 1),
        other => panic!("expected untagged sequence, got {other:?}"),
    }
}

#[test]
fn mixed_expr_accepts_template_tagged_struct() {
    init_tracing();
    let parsed: Doc = from_str(
        r#"
v @template{
    params ({text @Any})
    body {
        syntax text
        highlight keyword
    }
}
"#,
    )
    .unwrap();

    match parsed.v {
        Expr::Template(template) => {
            assert_eq!(template.params.len(), 1);
            assert_eq!(template.body.highlight.as_deref(), Some("keyword"));
        }
        other => panic!("expected template, got {other:?}"),
    }
}
