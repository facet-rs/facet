use facet::Facet;

facet_json_legacy::test_modes! {
    #[test]
    fn test_from_json_with_option() {
        #[derive(Facet, Debug, PartialEq)]
        struct Options {
            name: Option<String>,
            age: Option<u32>,
            inner: Option<Inner>,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            foo: i32,
        }

        let json = r#"{
            "name": "Alice",
            "age": null,
            "inner": {
                "foo": 42
            }
        }"#;

        let test_struct: Options = deserialize(json).unwrap();
        assert_eq!(test_struct.name.as_deref(), Some("Alice"));
        assert_eq!(test_struct.age, None);
        assert_eq!(test_struct.inner.as_ref().map(|i| i.foo), Some(42));
    }

    #[test]
    fn test_from_json_with_nested_options() {
        #[derive(Facet, Debug)]
        struct Options {
            name: Option<Option<String>>,
            age: Option<Box<u32>>,
            inner: Option<Box<Option<Inner>>>,
        }

        #[derive(Facet, Debug)]
        struct Inner {
            foo: i32,
        }

        let json = r#"{
            "name": "Alice",
            "age": 5,
            "inner": {
                "foo": 42
            }
        }"#;

        let test_struct: Options = deserialize(json).unwrap();
        assert_eq!(test_struct.name.flatten().as_deref(), Some("Alice"));
        assert_eq!(test_struct.age, Some(Box::new(5)));
        assert_eq!(
            test_struct
                .inner
                .and_then(|inner| inner.map(|inner| inner.foo)),
            Some(42)
        );
    }

    #[test]
    fn test_missing_option_defaults_to_none() {
        #[derive(Facet, Debug, PartialEq)]
        struct Options {
            present: i32,
            missing_name: Option<String>,
        }

        let json = r#"{"present": 7}"#;
        let test_struct: Options = deserialize(json).unwrap();
        assert_eq!(test_struct.present, 7);
        assert_eq!(test_struct.missing_name, None);
    }
}
