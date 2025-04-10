use facet_derive::Facet;
use facet_json_write::to_json;
use facet_poke::Peek;

use facet_core as facet;

#[ctor::ctor]
fn init() {
    // Initialize color backtrace for pretty stack traces
    color_backtrace::install();

    // Initialize logger to print all log levels
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .format_timestamp(None)
        .init();

    log::info!("Logging and color backtrace initialized");
}

#[test]
fn test_to_json() {
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct LinearFunction {
        variable: String,
        slope: f32,
        intercept: i32,
    }

    let test_struct = LinearFunction {
        variable: "x".to_string(),
        slope: -3.5,
        intercept: -5,
    };

    let _expected_json = r#"{"variable":"x","slope":-3.5,"intercept":-5}"#;
    let expected_json_indented = r#"{
  "variable": "x",
  "slope": -3.5,
  "intercept": -5
}"#;

    let mut buffer = Vec::new();
    let peek = Peek::new(&test_struct);
    to_json(peek, &mut buffer, true).unwrap();
    let json = String::from_utf8(buffer).unwrap();
    assert_eq!(json, expected_json_indented);
}
