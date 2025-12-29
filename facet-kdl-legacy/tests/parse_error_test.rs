use facet::Facet;
use facet_kdl_legacy as fkdl;
use miette::Diagnostic;

#[test]
fn test_kdl_booleans() {
    let inputs = [
        "foo true",
        "foo false",
        "foo #true",
        "foo #false",
        r#"foo "true""#,
    ];

    for input in inputs {
        let result = input.parse::<kdl::KdlDocument>();
        println!("{:30} -> {:?}", input, result.is_ok());
        if let Err(e) = &result {
            for d in &e.diagnostics {
                println!("   Error: {:?}", d.message);
            }
        }
    }
}

/// Test that KDL parse errors preserve the underlying diagnostic information.
/// This ensures that when the kdl crate returns rich error diagnostics,
/// facet-kdl properly exposes them through miette::Diagnostic.
#[test]
fn parse_error_preserves_diagnostics() {
    #[derive(Debug, Facet)]
    struct Config {
        #[facet(fkdl::child)]
        node: Node,
    }

    #[derive(Debug, Facet)]
    struct Node {
        #[facet(fkdl::argument)]
        value: bool,
    }

    // This KDL is invalid - "true" without # is not a valid boolean in KDL 2.0
    let input = r#"node true"#;

    let result: Result<Config, _> = facet_kdl_legacy::from_str(input);
    assert!(result.is_err());

    let err = result.unwrap_err();

    // The error should have source_code (from the kdl error)
    assert!(
        err.source_code().is_some(),
        "Parse error should expose source_code from kdl::KdlError"
    );

    // The error should have related diagnostics (the actual parse errors)
    let related: Vec<_> = err.related().into_iter().flatten().collect();
    assert!(
        !related.is_empty(),
        "Parse error should expose related diagnostics from kdl::KdlError"
    );

    // Verify we can render this with miette
    use miette::{GraphicalReportHandler, GraphicalTheme};
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut output, &err).unwrap();

    println!("Parse error diagnostic:\n{output}");

    // The rendered output should contain useful information about the parse error
    // (not just "Failed to parse KDL document")
    assert!(
        output.contains("true") || output.contains("identifier") || output.contains("Expected"),
        "Rendered error should contain details about the parse failure, got:\n{output}"
    );
}
