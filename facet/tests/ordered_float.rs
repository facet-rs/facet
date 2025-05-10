#![cfg(feature = "ordered-float")]

use facet::Facet;
use ordered_float::{NotNan, OrderedFloat};

#[test]
fn test_ordered_float() {
    let shape = <OrderedFloat<f64> as Facet>::SHAPE;
    assert!(shape.inner.is_some());

    let inner_shape = (shape.inner.unwrap())();
    assert_eq!(inner_shape.id, <f64 as Facet>::SHAPE.id);
}

#[test]
fn test_not_nan() {
    let shape = <NotNan<f64> as Facet>::SHAPE;
    assert!(shape.inner.is_some());

    let inner_shape = (shape.inner.unwrap())();
    assert_eq!(inner_shape.id, <f64 as Facet>::SHAPE.id);
}
