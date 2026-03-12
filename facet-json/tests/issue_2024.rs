//! Access violation when deserializing Arc<Wrapper> where Wrapper is transparent
//! over a proxy type, inside a flattened tagged enum.
//! Regression test for https://github.com/facet-rs/facet/issues/2024

use std::sync::Arc;

use facet::Facet;
use facet_testhelpers::test;

#[derive(Clone, Debug, Facet)]
#[facet(proxy = InnerProxy)]
pub struct Inner {
    a: Vec<f64>,
    b: Vec<f64>,
}

#[derive(Debug, Facet)]
pub struct InnerProxy {
    a: Vec<f64>,
    b: Vec<f64>,
}

impl TryFrom<InnerProxy> for Inner {
    type Error = String;

    fn try_from(p: InnerProxy) -> Result<Self, Self::Error> {
        Ok(Inner { a: p.a, b: p.b })
    }
}

impl From<&Inner> for InnerProxy {
    fn from(i: &Inner) -> Self {
        Self {
            a: i.a.clone(),
            b: i.b.clone(),
        }
    }
}

#[derive(Clone, Debug, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct Wrapper(Inner);

#[derive(Clone, Debug, Facet)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum TaggedEnum {
    A { data: Arc<Wrapper> },
}

#[derive(Clone, Debug, Facet)]
pub struct Outer {
    #[facet(flatten)]
    pub inner: TaggedEnum,
}

#[test]
fn test_issue_2024_proxy_transparent_arc_flatten() {
    let json = r#"{"kind": "A", "data": { "a": [0.0], "b": [1.0] }}"#;

    for i in 0..100 {
        let outer =
            facet_json::from_str::<Outer>(json).unwrap_or_else(|e| panic!("iteration {i}: {e}"));
        match &outer.inner {
            TaggedEnum::A { data } => {
                assert_eq!(data.0.a, vec![0.0], "iteration {i}: field a mismatch");
                assert_eq!(data.0.b, vec![1.0], "iteration {i}: field b mismatch");
            }
        }
    }
}
