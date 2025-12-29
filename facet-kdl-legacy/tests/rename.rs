use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

/// Test rename_all = "kebab-case" on structs for field-to-node matching.
/// This allows Rust snake_case fields to match kebab-case KDL node names.
#[test]
fn struct_rename_all_kebab_case() {
    #[derive(Facet, PartialEq, Debug)]
    #[facet(rename_all = "kebab-case")]
    struct Config {
        #[facet(kdl::child)]
        database_url: DatabaseUrl,
        #[facet(kdl::child)]
        #[facet(default)]
        max_connections: Option<MaxConnections>,
        #[facet(kdl::child)]
        #[facet(default)]
        retry_policy: Option<RetryPolicy>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct DatabaseUrl {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct MaxConnections {
        #[facet(kdl::argument)]
        value: u32,
    }

    #[derive(Facet, PartialEq, Debug)]
    #[facet(rename_all = "kebab-case")]
    struct RetryPolicy {
        #[facet(kdl::property)]
        max_retries: u32,
        #[facet(kdl::property)]
        backoff_ms: u32,
    }

    let kdl = indoc! {r#"
        database-url "postgres://localhost/mydb"
        max-connections 100
        retry-policy max-retries=3 backoff-ms=1000
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(config.database_url.value, "postgres://localhost/mydb");
    assert_eq!(config.max_connections.unwrap().value, 100);
    let retry = config.retry_policy.unwrap();
    assert_eq!(retry.max_retries, 3);
    assert_eq!(retry.backoff_ms, 1000);
}
