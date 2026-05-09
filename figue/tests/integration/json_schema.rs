use facet::Facet;
use figue::{self as args, builder};
use std::fs;

#[derive(Facet, Debug)]
struct Args {
    /// Primary configuration
    #[facet(args::config, args::env_prefix = "APP", rename = "cfg")]
    cfg: AppConfig,

    /// Evaluation configuration
    #[facet(args::config, rename = "eval")]
    eval: EvalConfig,
}

/// Application config root.
#[derive(Facet, Debug)]
struct AppConfig {
    /// Server hostname.
    #[facet(default = "localhost")]
    host: String,

    /// Server port.
    #[facet(default = 8080)]
    port: u16,

    /// Maximum retry attempts.
    #[facet(rename = "max-retries", default = 3)]
    max_retries: u32,

    /// Additional hostnames.
    aliases: Vec<String>,

    /// Optional TLS settings.
    #[facet(default)]
    tls: Option<TlsConfig>,

    /// Storage backend.
    storage: Storage,

    /// Log output format.
    #[facet(default)]
    format: LogFormat,
}

#[derive(Facet, Debug)]
struct TlsConfig {
    /// Certificate path.
    cert_path: String,

    /// Private key path.
    key_path: String,
}

#[derive(Facet, Debug)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
#[allow(dead_code)]
enum Storage {
    /// Local filesystem storage.
    Local {
        /// Base path.
        path: String,
    },
    /// S3-compatible storage.
    S3 {
        /// Bucket name.
        bucket: String,
        /// Region name.
        #[facet(default = "us-east-1")]
        region: String,
    },
    /// In-memory storage.
    Memory,
}

#[derive(Facet, Debug, Default)]
#[repr(u8)]
enum LogFormat {
    /// Plain text logs.
    #[default]
    Plain,
    /// JSON logs.
    Json,
}

#[derive(Facet, Debug)]
struct EvalConfig {
    /// Dataset name.
    dataset: String,

    /// Sample count.
    #[facet(default = 10)]
    samples: u32,
}

#[test]
fn test_generate_json_schemas_for_all_config_roots() {
    let schemas = figue::generate_json_schemas::<Args>().unwrap();

    assert_eq!(schemas.len(), 2);
    assert_eq!(schemas[0].file_name, "cfg.schema.json");
    assert_eq!(schemas[1].file_name, "eval.schema.json");

    insta::assert_snapshot!("cfg_json_schema", &schemas[0].contents);
    insta::assert_snapshot!("eval_json_schema", &schemas[1].contents);
}

#[test]
fn test_builder_writes_json_schemas_to_directory() {
    let tempdir = tempfile::tempdir().unwrap();
    let config = builder::<Args>().unwrap();

    let paths = config.write_json_schemas(tempdir.path()).unwrap();

    assert_eq!(paths.len(), 2);
    assert!(tempdir.path().join("cfg.schema.json").exists());
    assert!(tempdir.path().join("eval.schema.json").exists());

    let cfg = fs::read_to_string(tempdir.path().join("cfg.schema.json")).unwrap();
    assert!(cfg.contains(r#""$schema": "https://json-schema.org/draft/2020-12/schema""#));
    assert!(cfg.contains(r#""description": "Server hostname.""#));
}
