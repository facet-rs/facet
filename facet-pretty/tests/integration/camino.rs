#![cfg(feature = "camino")]

use camino::{Utf8Path, Utf8PathBuf};
use facet::Facet;
use facet_pretty::PrettyPrinter;
use facet_testhelpers::test;

#[derive(Facet)]
struct TestPathBuf {
    base: Utf8PathBuf,
}

#[derive(Facet)]
struct TestPath<'data> {
    reference: &'data Utf8Path,
}

#[test]
fn test_camino_simple() {
    let test_pathbuf = TestPathBuf {
        base: "/somewhere/over/the/rainbow".into(),
    };
    let formatted = PrettyPrinter::new().format(&test_pathbuf);

    assert!(formatted.contains("base"));
    assert!(formatted.contains("/somewhere/over/the/rainbow"));

    let test_path = TestPath {
        reference: &test_pathbuf.base,
    };
    let formatted = PrettyPrinter::new().format(&test_path);

    assert!(formatted.contains("reference"));
    assert!(formatted.contains("/somewhere/over/the/rainbow"));
}
