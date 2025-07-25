//! Tests for IP address deserialization from YAML

use eyre::Result;
use facet::Facet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Facet, PartialEq)]
struct Ipv4Root {
    value: Ipv4Addr,
}

#[derive(Debug, Facet, PartialEq)]
struct Ipv6Root {
    value: Ipv6Addr,
}

#[derive(Debug, Facet, PartialEq)]
struct IpRoot {
    value: IpAddr,
}

#[test]
fn test_ipv4_addr_deserialize() -> Result<()> {
    let yaml = "value: '127.0.0.1'";
    let result: Ipv4Root = facet_yaml::from_str(yaml)?;

    assert_eq!(
        result,
        Ipv4Root {
            value: "127.0.0.1".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
fn test_ipv4_addr_deserialize_unquoted() -> Result<()> {
    let yaml = "value: 192.168.1.1";
    let result: Ipv4Root = facet_yaml::from_str(yaml)?;

    assert_eq!(
        result,
        Ipv4Root {
            value: "192.168.1.1".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
fn test_ipv6_addr_deserialize() -> Result<()> {
    let yaml = "value: '::1'";
    let result: Ipv6Root = facet_yaml::from_str(yaml)?;

    assert_eq!(
        result,
        Ipv6Root {
            value: "::1".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
fn test_ipv6_addr_deserialize_full() -> Result<()> {
    let yaml = "value: '2001:0db8:85a3:0000:0000:8a2e:0370:7334'";
    let result: Ipv6Root = facet_yaml::from_str(yaml)?;

    assert_eq!(
        result,
        Ipv6Root {
            value: "2001:0db8:85a3:0000:0000:8a2e:0370:7334".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
fn test_ip_addr_deserialize_v4() -> Result<()> {
    let yaml = "value: '10.0.0.1'";
    let result: IpRoot = facet_yaml::from_str(yaml)?;

    assert_eq!(
        result,
        IpRoot {
            value: "10.0.0.1".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
fn test_ip_addr_deserialize_v6() -> Result<()> {
    let yaml = "value: '::ffff:192.0.2.1'";
    let result: IpRoot = facet_yaml::from_str(yaml)?;

    assert_eq!(
        result,
        IpRoot {
            value: "::ffff:192.0.2.1".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
fn test_ipv4_addr_deserialize_invalid() {
    let yaml = "value: 'not.an.ip.address'";
    let result = facet_yaml::from_str::<Ipv4Root>(yaml);

    assert!(result.is_err(), "Should fail to parse invalid IP address");
}

#[test]
fn test_ipv6_addr_deserialize_invalid() {
    let yaml = "value: 'definitely not an ipv6'";
    let result = facet_yaml::from_str::<Ipv6Root>(yaml);

    assert!(result.is_err(), "Should fail to parse invalid IPv6 address");
}
