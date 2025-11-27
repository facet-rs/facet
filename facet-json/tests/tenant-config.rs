use camino::Utf8PathBuf;
use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

/// tenant-specific configuration that's common betweeen mom and cub
#[derive(Facet, Debug)]
pub struct TenantConfig {
    pub name: String,
    pub domain_aliases: Vec<String>,
    pub object_storage: Option<ObjectStorageConfig>,
    pub secrets: Option<TenantSecrets>,
    pub base_dir_for_dev: Option<Utf8PathBuf>,
}

#[derive(Facet, Debug)]
pub struct ObjectStorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
}
#[derive(Facet, Debug)]
pub struct TenantSecrets {
    pub aws: AwsSecrets,
    pub patreon: Option<PatreonSecrets>,
    pub github: Option<GitHubSecrets>,
    /// Derived cookie sauce for this tenant (derived from global secret)
    #[facet(optional)]
    pub cookie_sauce: Option<String>,
}

#[derive(Facet, Debug)]
pub struct AwsSecrets {
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Facet, Clone, Debug)]
pub struct PatreonSecrets {
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

#[derive(Facet, Clone, Debug)]
pub struct GitHubSecrets {
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

#[test]
fn tenantconfig_wrong_free() {
    let input = r#"
    [
      {
        "name": "fasterthanli.me",
        "object_storage": {
          "bucket": "[REDACTED]",
          "region": "[REDACTED]",
          "endpoint": "[REDACTED]"
        },
        "secrets": {
          "aws": {
            "access_key_id": "[REDACTED]",
            "secret_access_key": "[REDACTED]"
          },
          "patreon": {
            "oauth_client_id": "[REDACTED]",
            "oauth_client_secret": "[REDACTED]"
          },
          "github": {
            "oauth_client_id": "[REDACTED]",
            "oauth_client_secret": "[REDACTED]"
          }
        }
      }
    ]
    "#;

    let result = from_str::<Vec<TenantConfig>>(input);
    #[cfg(not(miri))]
    insta::assert_debug_snapshot!(result);
}
