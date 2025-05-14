use facet::Facet;
use facet_testhelpers::test;
use indoc::indoc;

#[test]
#[ignore]
fn basic_node() {
    // QUESTION: I don't know when this would be particularly good practice, but it could be nice if `facet` shipped
    // some sort of macro that allowed libraries to rename the Facet trait / attributes? This might make it clearer
    // what's going on if you're ever mixing several `Facet` libraries that all use different arbitrary attributes? I
    // just think that `#[kdl(child)]` would be a lot clearer than `#[facet(child)]` if, say, you also wanted to
    // deserialize from something like XML? Or command-line arguments? Those would also need attributes, e.g.
    // `#[facet(text)]` or `#[facet(positional)]`, and I think things would be a lot clearer as `#[xml(text)]` and
    // `#[args(positional)]`. If, however, it's far too evil or hard to implment something like that, then arbitrary
    // attributes should be given "namespaces", maybe? Like `#[facet(kdl, child)]` or `#[facet(xml, text)]?
    //
    // Overall I think this is a hard design question, but I do think it's worth considering how several `facet` crates
    // relying on arbitrary attributes should interact...
    #[derive(Facet)]
    struct Basic {
        #[facet(child)]
        title: Title,
    }

    #[derive(Facet)]
    struct Title {
        #[facet(argument)]
        title: String,
    }

    let kdl = indoc! {r#"
        title "Hello, World"
    "#};

    dbg!(Basic::SHAPE);

    let _basic: Basic = facet_kdl::from_str(kdl)?;
}
