use facet::Facet;
use facet_kdl as kdl;
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};

#[derive(Facet)]
struct ServerConfig {
    #[facet(kdl::child)]
    server: Server,
}

#[derive(Facet)]
struct Server {
    #[facet(kdl::argument)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

// For testing ExpectedScalarGotStruct error
#[derive(Facet)]
struct RustConfigWrapper {
    #[facet(kdl::child)]
    rust: RustConfig,
}

#[derive(Facet)]
struct RustConfig {
    #[facet(kdl::property)]
    command: Option<String>,
    #[facet(kdl::property)]
    args: Option<String>,
}

fn print_error(name: &str, e: &kdl::KdlDeserializeError) {
    println!("\n{:=^60}", "");
    println!("=== {} ===", name);
    println!("{:=^60}", "");
    println!("Display: {}", e);
    println!("labels: {}", e.labels().is_some());
    println!("related: {}", e.related().is_some());

    println!("\n=== Miette render ===");
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut output, e).unwrap();
    println!("{}", output);
}

fn main() {
    // Test 1: Parse error (syntax error in KDL)
    let parse_error_input = r#"server "localhost port=8080"#;
    if let Err(e) = kdl::from_str_rich::<ServerConfig>(parse_error_input) {
        print_error("Parse Error (unclosed quote)", &e);
    }

    // Test 2: Type error (missing required field)
    let type_error_input = r#"server "localhost""#;
    if let Err(e) = kdl::from_str_rich::<ServerConfig>(type_error_input) {
        print_error("Type Error (missing port)", &e);
    }

    // Test 3: ExpectedScalarGotStruct (using child node syntax for property field)
    // This is the case where user writes `command "cargo"` instead of `command="cargo"`
    let scalar_struct_input = r#"rust {
    command "cargo"
    args "run" "--quiet" "--release"
}"#;
    if let Err(e) = kdl::from_str_rich::<RustConfigWrapper>(scalar_struct_input) {
        print_error("ExpectedScalarGotStruct", &e);
    }
}
