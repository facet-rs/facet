use facet::Facet;
use facet_yaml_legacy::Spanned;

#[test]
fn spanned_values() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Config {
        server: Server,
    }

    #[derive(Facet, Debug)]
    struct Server {
        host: Spanned<String>,
        port: Spanned<u32>,
    }

    let yaml = "server:\n  host: localhost\n  port: 8080";

    let config: Config = facet_yaml_legacy::from_str(yaml).unwrap();

    // Check that values are correct
    assert_eq!(*config.server.host, "localhost");
    assert_eq!(*config.server.port, 8080);

    // Check that spans are populated (not unknown)
    assert!(!config.server.host.span().is_unknown());
    assert!(!config.server.port.span().is_unknown());

    // The host span should cover "localhost"
    let host_span = config.server.host.span();
    let port_span = config.server.port.span();

    // Host comes before port in the source
    assert!(host_span.offset < port_span.offset);

    println!("Host span: {host_span:?}");
    println!("Port span: {port_span:?}");

    // Verify the spans point to the actual values in the source
    let host_text = &yaml[host_span.offset..host_span.offset + host_span.len];
    assert_eq!(host_text, "localhost");

    let port_text = &yaml[port_span.offset..port_span.offset + port_span.len];
    assert_eq!(port_text, "8080");
}

#[test]
fn spanned_in_list() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Config {
        ports: Vec<Spanned<u16>>,
    }

    let yaml = "ports:\n  - 80\n  - 443\n  - 8080";

    let config: Config = facet_yaml_legacy::from_str(yaml).unwrap();

    assert_eq!(config.ports.len(), 3);
    assert_eq!(*config.ports[0], 80);
    assert_eq!(*config.ports[1], 443);
    assert_eq!(*config.ports[2], 8080);

    // All spans should be valid
    for port in &config.ports {
        assert!(!port.span().is_unknown());
    }

    // Spans should be in order
    assert!(config.ports[0].span().offset < config.ports[1].span().offset);
    assert!(config.ports[1].span().offset < config.ports[2].span().offset);
}
