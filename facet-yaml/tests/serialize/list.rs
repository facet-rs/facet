//! Tests for YAML values to lists.

use eyre::Result;
use facet::Facet;

use crate::assert_serialize;

#[test]
fn test_scalar_list() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<i32>,
    }

    assert_serialize!(Root, Root { values: Vec::new() },);

    assert_serialize!(Root, Root { values: vec![2] },);

    assert_serialize!(
        Root,
        Root {
            values: vec![1, -1, 0, 100],
        },
    );

    Ok(())
}

#[test]
#[ignore = "Must be fixed in facet-yaml deserialize"]
fn test_option_scalar_list() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Option<i64>>,
    }

    assert_serialize!(Root, Root { values: Vec::new() },);

    assert_serialize!(
        Root,
        Root {
            values: vec![Some(2)]
        }
    );
    assert_serialize!(
        Root,
        Root {
            values: vec![Some(2), Some(3)]
        }
    );
    assert_serialize!(
        Root,
        Root {
            values: vec![Some(2), None, Some(3)]
        }
    );

    Ok(())
}
