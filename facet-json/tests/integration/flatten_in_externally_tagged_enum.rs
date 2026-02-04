//! Test for flattened fields in externally-tagged enum struct variants.
//! See https://github.com/facet-rs/facet/issues/1921

use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

#[derive(Facet, Debug, PartialEq, Default)]
struct LoggingOpts {
    debug: bool,
    #[facet(default)]
    log_file: Option<String>,
}

#[derive(Facet, Debug, PartialEq, Default)]
struct CommonOpts {
    verbose: bool,
    quiet: bool,
    #[facet(flatten)]
    logging: LoggingOpts,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum DeepCommand {
    Execute {
        #[facet(flatten)]
        common: CommonOpts,
        target: String,
    },
}

#[test]
fn test_flatten_in_externally_tagged_enum_variant() {
    // JSON with flat fields - this is what figue's ConfigValue produces
    let json = r#"{"Execute": {"verbose": true, "debug": true, "log_file": "/var/log/app.log", "target": "my-target", "quiet": false}}"#;

    let result: DeepCommand = from_str(json).expect("should deserialize");

    match &result {
        DeepCommand::Execute { common, target } => {
            assert!(common.verbose, "verbose should be true");
            assert!(!common.quiet, "quiet should be false");
            assert!(common.logging.debug, "debug should be true");
            assert_eq!(common.logging.log_file.as_deref(), Some("/var/log/app.log"));
            assert_eq!(target, "my-target");
        }
    }
}

#[test]
fn test_flatten_in_externally_tagged_enum_with_defaults() {
    // JSON with only required fields - defaults should apply
    let json = r#"{"Execute": {"target": "simple-target"}}"#;

    let result: DeepCommand = from_str(json).expect("should deserialize");

    match &result {
        DeepCommand::Execute { common, target } => {
            assert!(!common.verbose, "verbose should default to false");
            assert!(!common.quiet, "quiet should default to false");
            assert!(!common.logging.debug, "debug should default to false");
            assert_eq!(common.logging.log_file, None);
            assert_eq!(target, "simple-target");
        }
    }
}
