//! FieldPath operations tests.

use facet_solver::{FieldPath, PathSegment};
use facet_testhelpers::test;

#[test]
fn test_field_path_operations() {
    let path = FieldPath::empty();
    assert_eq!(path.depth(), 0);

    let path = path.push_field("foo");
    assert_eq!(path.depth(), 1);
    assert_eq!(path.segments(), &[PathSegment::Field("foo")]);

    let path = path.push_field("bar");
    assert_eq!(path.depth(), 2);

    let parent = path.parent();
    assert_eq!(parent.depth(), 1);

    let path = path.push_variant("config", "Advanced");
    assert_eq!(path.depth(), 3);
    assert_eq!(
        path.last(),
        Some(&PathSegment::Variant("config", "Advanced"))
    );
}
