use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn msgpack_deserialize_empty_vec() -> Result<()> {
    facet_testhelpers::setup();

    let data = [
        0x90, // Array with 0 elements
    ];

    let v: Vec<i32> = from_slice(&data)?;
    assert_eq!(v, vec![]);

    Ok(())
}

#[test]
fn msgpack_deserialize_vec() -> Result<()> {
    facet_testhelpers::setup();

    let data = [
        0x95, // Array with 5 elements
        0x01, 0x02, 0x03, 0x04, 0x05, // 1, 2, 3, 4, 5
    ];

    let v: Vec<u64> = from_slice(&data)?;
    assert_eq!(v, vec![1, 2, 3, 4, 5]);

    Ok(())
}

#[test]
fn test_nested_arrays() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Clone, Default)]
    pub struct NestedArrays {
        pub matrix: Vec<Vec<u64>>,
    }

    // { matrix: [[1, 2, 3], [], [4, 5]] }
    let data = [
        0x81, // Map with 1 element
        0xa6, 0x6d, 0x61, 0x74, 0x72, 0x69, 0x78, // "matrix"
        0x93, // Array with 3 elements
        0x93, // Array with 3 elements
        0x01, 0x02, 0x03, // 1, 2, 3
        0x90, // Empty array
        0x92, // Array with 2 elements
        0x04, 0x05, // 4, 5
    ];

    let nested: NestedArrays = from_slice(&data)?;
    assert_eq!(nested.matrix.len(), 3);
    assert_eq!(nested.matrix[0], vec![1, 2, 3]);
    assert_eq!(nested.matrix[1], vec![]);
    assert_eq!(nested.matrix[2], vec![4, 5]);

    Ok(())
}
