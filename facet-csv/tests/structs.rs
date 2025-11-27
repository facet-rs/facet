use facet_testhelpers::test;

#[test]
fn test_writing_flat_structs() {
    #[derive(facet::Facet)]
    struct MyStruct {
        value1: usize,
        value2: &'static str,
        value3: bool,
    }

    let expected_mystruct = "1,some,false\n";
    let actual = facet_csv::to_string(&MyStruct {
        value1: 1,
        value2: "some",
        value3: false,
    });
    assert_eq!(expected_mystruct, actual);
}
