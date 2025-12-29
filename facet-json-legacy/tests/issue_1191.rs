/// Test for issue #1191: Deserializing a multilevel transparent struct fails
///
/// This test reproduces the case where:
/// - A generic struct `GCurve<X, Y>` uses a proxy for deserialization
/// - A transparent struct `Curve64` wraps `GCurve<f64, f64>`
/// - An untagged enum `CurveRepr` has a variant containing `Curve64`
///
/// The issue is that the solver doesn't properly look through transparent
/// wrappers when determining which variant's fields match the JSON keys.
use facet::Facet;
use facet_json_legacy as json;

/// The underlying generic curve type that uses a proxy for deserialization
#[derive(Clone, Debug, PartialEq, Facet)]
#[facet(proxy = GCurveProxy<X, Y>)]
pub struct GCurve<X: Clone + 'static, Y: Clone + 'static> {
    pub x: Vec<X>,
    pub y: Vec<Y>,
}

/// The proxy type used for GCurve deserialization
#[derive(Debug, Facet)]
pub struct GCurveProxy<X: 'static, Y: 'static> {
    pub x: Vec<X>,
    pub y: Vec<Y>,
}

impl<X: 'static, Y: 'static> TryFrom<GCurveProxy<X, Y>> for GCurve<X, Y>
where
    X: Clone,
    Y: Clone,
{
    type Error = String;

    fn try_from(c: GCurveProxy<X, Y>) -> Result<Self, Self::Error> {
        Ok(GCurve { x: c.x, y: c.y })
    }
}

impl<X: Clone + 'static, Y: Clone + 'static> From<&GCurve<X, Y>> for GCurveProxy<X, Y> {
    fn from(c: &GCurve<X, Y>) -> Self {
        GCurveProxy {
            x: c.x.clone(),
            y: c.y.clone(),
        }
    }
}

/// A transparent wrapper around GCurve<f64, f64>
#[derive(Debug, PartialEq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct Curve64(pub GCurve<f64, f64>);

/// An untagged enum with one variant containing the transparent Curve64
#[derive(Debug, PartialEq, Facet)]
#[facet(untagged)]
#[repr(C)]
pub enum CurveRepr {
    Linear(Curve64),
    Constant { constant: f64 },
    Special { special: String },
}

#[test]
fn test_multilevel_transparent_in_untagged_enum() {
    // This JSON should deserialize as CurveRepr::Linear because it has "x" and "y" fields
    // which match the GCurveProxy<f64, f64> structure that Curve64 wraps
    let json = r#"{"x":[0.0,1.0],"y":[0.22,0.25]}"#;

    let result: CurveRepr = json::from_str(json).expect("should deserialize as Linear variant");

    match result {
        CurveRepr::Linear(curve) => {
            assert_eq!(curve.0.x, vec![0.0, 1.0]);
            assert_eq!(curve.0.y, vec![0.22, 0.25]);
        }
        _ => panic!("Expected Linear variant, got {:?}", result),
    }
}

#[test]
fn test_constant_variant_still_works() {
    let json = r#"{"constant":42.5}"#;

    let result: CurveRepr = json::from_str(json).expect("should deserialize as Constant variant");

    assert_eq!(result, CurveRepr::Constant { constant: 42.5 });
}

#[test]
fn test_special_variant_still_works() {
    let json = r#"{"special":"custom"}"#;

    let result: CurveRepr = json::from_str(json).expect("should deserialize as Special variant");

    assert_eq!(
        result,
        CurveRepr::Special {
            special: "custom".to_string()
        }
    );
}
