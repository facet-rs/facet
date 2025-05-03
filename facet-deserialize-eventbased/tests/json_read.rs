use facet::Facet;
use facet_deserialize_eventbased::Json;

fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(input: &'input str) -> eyre::Result<T> {
    let input = input.as_bytes();
    Ok(facet_deserialize_eventbased::deserialize::<T, Json>(input).map_err(|e| e.into_owned())?)
}

#[test]
fn json_simple_object() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct FooBar {
        foo: u32,
        bar: String,
    }

    let fb: FooBar = from_str(r#"{ "foo": 123456, "bar": "Hello, World!" }"#)?;
    assert_eq!(fb.foo, 123456);
    assert_eq!(fb.bar, "Hello, World!");

    Ok(())
}

#[test]
fn test_one_empty_one_nonempty_vec() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": [],
      "two": ["a", "b", "c"]
    }
    "#;

    let config: RevisionConfig = from_str(markup)?;
    assert!(config.one.is_empty());
    assert_eq!(config.two, vec!["a", "b", "c"]);

    Ok(())
}
