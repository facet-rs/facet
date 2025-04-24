// Basic tests go here

use facet::Facet;
use indoc::indoc;

#[test]
fn basic_node() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Basic {
        // FIXME: This is wrong, should be `child, unwrap(argument)`?
        #[facet(argument)]
        title: String,
    }

    let kdl = indoc! {r#"
        title "Hello, World"
    "#};

    dbg!(Basic::SHAPE);

    let basic: Basic = facet_kdl::from_str(kdl).unwrap();
}
