use facet::Facet;

#[test]
fn nested_struct_invariants_are_enforced() {
    #[derive(Debug, Facet)]
    struct TopLevel {
        point: Point,
    }

    #[derive(Debug, Facet)]
    #[facet(invariants = is_valid)]
    struct Point {
        x: i32,
        y: i32,
    }

    fn is_valid(point: &Point) -> bool {
        point.x >= 0 && point.y >= 0
    }

    let ok: TopLevel = facet_json::from_str(r#"{ "point": { "x": 5, "y": 12 } }"#).unwrap();
    assert_eq!(ok.point.x, 5);
    assert_eq!(ok.point.y, 12);

    let bad: Result<TopLevel, _> = facet_json::from_str(r#"{ "point": { "x": -25, "y": 12 } }"#);
    assert!(bad.is_err());
}
