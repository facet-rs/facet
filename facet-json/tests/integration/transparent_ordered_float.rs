use facet::Facet;
use ordered_float::OrderedFloat;

/**
 * This test verifies that Facet can properly serialize and deserialize
 * enum struct variants.
 */

#[test]
fn transparent_ordered_float_1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[facet(transparent)]
    struct PixelDensity(f32);

    let deser: PixelDensity = facet_json::from_str(r#"1.4"#).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(deser.0, 1.4);

    Ok(())
}

#[test]
fn transparent_ordered_float_2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    type PixelDensity = OrderedFloat<f32>;

    let deser: PixelDensity = facet_json::from_str(r#"1.4"#).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(deser.0, 1.4);

    Ok(())
}
