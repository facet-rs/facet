use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;

/// Tests for internally-tagged enums with per-variant `#[facet(untagged)]`.
///
/// This models the pattern from serde where `#[serde(tag = "type")]` on the enum
/// combined with `#[serde(untagged)]` on a variant creates a mixed dispatch:
/// tagged variants are matched by the tag field, and untagged variants act as
/// fallbacks when the tag is absent or doesn't match.

#[derive(Debug, Facet, PartialEq)]
struct McpServerHttp {
    name: String,
    url: String,
}

#[derive(Debug, Facet, PartialEq)]
struct McpServerSse {
    name: String,
    url: String,
}

#[derive(Debug, Facet, PartialEq)]
struct McpServerStdio {
    name: String,
    command: String,
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
#[facet(tag = "type", rename_all = "snake_case")]
enum McpServer {
    Http(McpServerHttp),
    Sse(McpServerSse),
    #[facet(untagged)]
    Stdio(McpServerStdio),
}

#[test]
fn deserialize_tagged_http() {
    let json = r#"{"type":"http","name":"my-server","url":"http://localhost:8080"}"#;
    let parsed: McpServer = from_str(json).unwrap();
    assert_eq!(
        parsed,
        McpServer::Http(McpServerHttp {
            name: "my-server".to_string(),
            url: "http://localhost:8080".to_string(),
        })
    );
}

#[test]
fn deserialize_tagged_sse() {
    let json = r#"{"type":"sse","name":"my-sse","url":"http://localhost:9090/events"}"#;
    let parsed: McpServer = from_str(json).unwrap();
    assert_eq!(
        parsed,
        McpServer::Sse(McpServerSse {
            name: "my-sse".to_string(),
            url: "http://localhost:9090/events".to_string(),
        })
    );
}

#[test]
fn deserialize_untagged_stdio_no_type_field() {
    let json = r#"{"name":"my-stdio","command":"npx server"}"#;
    let parsed: McpServer = from_str(json).unwrap();
    assert_eq!(
        parsed,
        McpServer::Stdio(McpServerStdio {
            name: "my-stdio".to_string(),
            command: "npx server".to_string(),
        })
    );
}

#[test]
fn serialize_tagged_http() {
    let server = McpServer::Http(McpServerHttp {
        name: "my-server".to_string(),
        url: "http://localhost:8080".to_string(),
    });
    let json = to_string(&server).unwrap();
    assert!(json.contains(r#""type":"http""#), "missing type tag");
    assert!(json.contains(r#""name":"my-server""#));
    assert!(json.contains(r#""url":"http://localhost:8080""#));
}

#[test]
fn serialize_untagged_stdio() {
    let server = McpServer::Stdio(McpServerStdio {
        name: "my-stdio".to_string(),
        command: "npx server".to_string(),
    });
    let json = to_string(&server).unwrap();
    // Untagged variant should NOT have a "type" field
    assert!(
        !json.contains(r#""type""#),
        "untagged variant should not have type field"
    );
    assert!(json.contains(r#""name":"my-stdio""#));
    assert!(json.contains(r#""command":"npx server""#));
}

#[test]
fn roundtrip_http() {
    let server = McpServer::Http(McpServerHttp {
        name: "test".to_string(),
        url: "http://example.com".to_string(),
    });
    let json = to_string(&server).unwrap();
    let parsed: McpServer = from_str(&json).unwrap();
    assert_eq!(parsed, server);
}

#[test]
fn roundtrip_sse() {
    let server = McpServer::Sse(McpServerSse {
        name: "test".to_string(),
        url: "http://example.com/sse".to_string(),
    });
    let json = to_string(&server).unwrap();
    let parsed: McpServer = from_str(&json).unwrap();
    assert_eq!(parsed, server);
}

#[test]
fn roundtrip_stdio() {
    let server = McpServer::Stdio(McpServerStdio {
        name: "test".to_string(),
        command: "my-command".to_string(),
    });
    let json = to_string(&server).unwrap();
    let parsed: McpServer = from_str(&json).unwrap();
    assert_eq!(parsed, server);
}

#[test]
fn deserialize_tag_order_doesnt_matter() {
    // Tag field comes after other fields
    let json = r#"{"name":"my-server","url":"http://localhost","type":"http"}"#;
    let parsed: McpServer = from_str(json).unwrap();
    assert_eq!(
        parsed,
        McpServer::Http(McpServerHttp {
            name: "my-server".to_string(),
            url: "http://localhost".to_string(),
        })
    );
}
