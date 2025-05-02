use facet::Facet;
use facet_deserialize_eventbased::Json;
use facet_reflect::Wip;

#[test]
fn test_json_read() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct FooBar {
        foo: u32,
        bar: String,
    }

    let wip = Wip::alloc_shape(FooBar::SHAPE)?;
    let input = r#"{ "foo": 123456, "bar": "Hello, World!" }"#.as_bytes();
    let _wip = facet_deserialize_eventbased::deserialize_wip::<Json>(wip, input)?;

    Ok(())
}
