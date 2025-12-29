use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};

#[test]
fn test_ipv4addr_roundtrip() {
    facet_testhelpers::setup();

    let addr = Ipv4Addr::new(192, 168, 1, 1);
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv4Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipv4addr_localhost() {
    facet_testhelpers::setup();

    let addr = Ipv4Addr::LOCALHOST; // 127.0.0.1
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv4Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipv4addr_unspecified() {
    facet_testhelpers::setup();

    let addr = Ipv4Addr::UNSPECIFIED; // 0.0.0.0
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv4Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipv4addr_broadcast() {
    facet_testhelpers::setup();

    let addr = Ipv4Addr::BROADCAST; // 255.255.255.255
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv4Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipv6addr_roundtrip() {
    facet_testhelpers::setup();

    let addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv6Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipv6addr_localhost() {
    facet_testhelpers::setup();

    let addr = Ipv6Addr::LOCALHOST; // ::1
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv6Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipv6addr_unspecified() {
    facet_testhelpers::setup();

    let addr = Ipv6Addr::UNSPECIFIED; // ::
    let bytes = to_vec(&addr).unwrap();
    let decoded: Ipv6Addr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipaddr_v4() {
    facet_testhelpers::setup();

    let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let bytes = to_vec(&addr).unwrap();
    let decoded: IpAddr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_ipaddr_v6() {
    facet_testhelpers::setup();

    let addr = IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
    let bytes = to_vec(&addr).unwrap();
    let decoded: IpAddr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_socketaddr_v4() {
    facet_testhelpers::setup();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let bytes = to_vec(&addr).unwrap();
    let decoded: SocketAddr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_socketaddr_v6() {
    facet_testhelpers::setup();

    let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 443);
    let bytes = to_vec(&addr).unwrap();
    let decoded: SocketAddr = from_slice(&bytes).unwrap();
    assert_eq!(addr, decoded);
}

#[test]
fn test_net_types_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct ServerConfig {
        listen_addr: SocketAddr,
        trusted_ips: Vec<IpAddr>,
    }

    let config = ServerConfig {
        listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
        trusted_ips: vec![
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
        ],
    };

    let bytes = to_vec(&config).unwrap();
    let decoded: ServerConfig = from_slice(&bytes).unwrap();
    assert_eq!(config, decoded);
}

#[test]
fn test_net_types_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct NetworkConfig {
        ipv4: Option<Ipv4Addr>,
        ipv6: Option<Ipv6Addr>,
    }

    let config = NetworkConfig {
        ipv4: Some(Ipv4Addr::new(192, 168, 1, 100)),
        ipv6: None,
    };

    let bytes = to_vec(&config).unwrap();
    let decoded: NetworkConfig = from_slice(&bytes).unwrap();
    assert_eq!(config, decoded);
}

#[test]
fn test_ipv4addr_serialization_size() {
    facet_testhelpers::setup();

    let addr = Ipv4Addr::new(192, 168, 1, 1);
    let bytes = to_vec(&addr).unwrap();
    // IPv4 should be exactly 4 bytes
    assert_eq!(bytes.len(), 4);
}

#[test]
fn test_ipv6addr_serialization_size() {
    facet_testhelpers::setup();

    let addr = Ipv6Addr::LOCALHOST;
    let bytes = to_vec(&addr).unwrap();
    // IPv6 should be exactly 16 bytes
    assert_eq!(bytes.len(), 16);
}

#[test]
fn test_ipaddr_v4_serialization_size() {
    facet_testhelpers::setup();

    let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let bytes = to_vec(&addr).unwrap();
    // 1 byte tag + 4 bytes for IPv4
    assert_eq!(bytes.len(), 5);
}

#[test]
fn test_ipaddr_v6_serialization_size() {
    facet_testhelpers::setup();

    let addr = IpAddr::V6(Ipv6Addr::LOCALHOST);
    let bytes = to_vec(&addr).unwrap();
    // 1 byte tag + 16 bytes for IPv6
    assert_eq!(bytes.len(), 17);
}

#[test]
fn test_socketaddr_v4_serialization_size() {
    facet_testhelpers::setup();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let bytes = to_vec(&addr).unwrap();
    // 1 byte tag + 4 bytes IP + 2 bytes port
    assert_eq!(bytes.len(), 7);
}

#[test]
fn test_socketaddr_v6_serialization_size() {
    facet_testhelpers::setup();

    let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 443);
    let bytes = to_vec(&addr).unwrap();
    // 1 byte tag + 16 bytes IP + 2 bytes port
    assert_eq!(bytes.len(), 19);
}

#[test]
fn test_multiple_net_types() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct MultiNet {
        v4: Ipv4Addr,
        v6: Ipv6Addr,
        ip: IpAddr,
        socket: SocketAddr,
    }

    let multi = MultiNet {
        v4: Ipv4Addr::new(192, 168, 1, 1),
        v6: Ipv6Addr::LOCALHOST,
        ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        socket: SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8080),
    };

    let bytes = to_vec(&multi).unwrap();
    let decoded: MultiNet = from_slice(&bytes).unwrap();
    assert_eq!(multi, decoded);
}

#[test]
fn test_socketaddr_different_ports() {
    facet_testhelpers::setup();

    for port in [0u16, 80, 443, 8080, 65535] {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let bytes = to_vec(&addr).unwrap();
        let decoded: SocketAddr = from_slice(&bytes).unwrap();
        assert_eq!(addr, decoded);
    }
}
