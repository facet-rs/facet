use facet::Facet;
use facet_kdl_legacy as kdl;
use facet_kdl_legacy::Spanned;
use indoc::indoc;

#[test]
fn spanned_values() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug)]
    struct Server {
        #[facet(kdl::argument)]
        host: Spanned<String>,
        #[facet(kdl::property)]
        port: Spanned<u32>,
    }

    let kdl = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();

    // Check that values are correct
    assert_eq!(*config.server.host, "localhost");
    assert_eq!(*config.server.port, 8080);

    // Check that spans are populated (not unknown)
    assert!(!config.server.host.span().is_unknown());
    assert!(!config.server.port.span().is_unknown());

    // The host span should cover "localhost" (including quotes in KDL)
    let host_span = config.server.host.span();
    let port_span = config.server.port.span();

    // Host comes before port in the source
    assert!(host_span.offset < port_span.offset);

    println!("Host span: {host_span:?}");
    println!("Port span: {port_span:?}");

    // Test round-trip: serialize and deserialize again
    // Note: spans won't be identical after round-trip (different source positions)
    let kdl_out = facet_kdl_legacy::to_string(&config).unwrap();
    println!("Serialized KDL:\n{kdl_out}");

    // The serialized output should contain the values
    assert!(kdl_out.contains("\"localhost\""));
    assert!(kdl_out.contains("port=8080"));

    // Parse it back - values should match
    let config2: Config = facet_kdl_legacy::from_str(&kdl_out).unwrap();
    assert_eq!(*config2.server.host, "localhost");
    assert_eq!(*config2.server.port, 8080);
}
