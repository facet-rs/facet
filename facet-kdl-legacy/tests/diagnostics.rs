use facet::Facet;
use facet_kdl_legacy as kdl;
use facet_kdl_legacy::{Span, Spanned};
use indoc::indoc;

// ============================================================================
// Miette diagnostics tests
// ============================================================================

/// Test that KDL parse errors produce useful error messages.
#[test]
fn kdl_parse_error() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
    }

    // Invalid KDL - unclosed brace
    let kdl = indoc! {r#"
        server "localhost" {
            nested "value"
        // missing closing brace
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_msg = format!("{err}");
    println!("Parse error: {err_msg}");

    // The error should mention something about the parse failure
    assert!(!err_msg.is_empty());
}

/// Test using Spanned to show semantic errors with source locations.
/// This demonstrates how to report "this value was invalid" after successful parsing.
#[test]
fn miette_semantic_error_with_spanned() {
    use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};
    use std::fmt;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug)]
    struct Server {
        #[facet(kdl::argument)]
        host: Spanned<String>,
        #[facet(kdl::property)]
        port: Spanned<u16>,
    }

    // Custom error type that uses miette for nice diagnostics
    #[derive(Debug)]
    struct ValidationError {
        src: String,
        span: Span,
        message: String,
    }

    impl fmt::Display for ValidationError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for ValidationError {}

    impl Diagnostic for ValidationError {
        fn source_code(&self) -> Option<&dyn miette::SourceCode> {
            Some(&self.src)
        }

        fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
            Some(Box::new(std::iter::once(miette::LabeledSpan::at(
                self.span.offset..self.span.end(),
                "invalid value here",
            ))))
        }

        fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            Some(Box::new("port must be between 1 and 65535"))
        }
    }

    let kdl_source = indoc! {r#"
        server "localhost" port=0
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_source).unwrap();

    // Simulate validation: port 0 is invalid
    let port_value = *config.server.port;
    let validation_result = if port_value == 0 {
        Err(ValidationError {
            src: kdl_source.to_string(),
            span: config.server.port.span(),
            message: format!("invalid port number: {port_value}"),
        })
    } else {
        Ok(())
    };

    assert!(validation_result.is_err());
    let err = validation_result.unwrap_err();

    // Render with miette (with colors!)
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut output, &err).unwrap();

    println!("Semantic error diagnostic:\n{output}");

    // The diagnostic should highlight the port=0 span
    assert!(output.contains("invalid"));
    assert!(output.contains("port"));
}

/// Test combining Spanned with multiple validation errors.
#[test]
fn miette_multiple_validation_errors() {
    use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};
    use std::fmt;

    #[derive(Facet, Debug)]
    struct Pipeline {
        #[facet(kdl::children)]
        tasks: Vec<Task>,
    }

    #[derive(Facet, Debug)]
    struct Task {
        #[facet(kdl::node_name)]
        name: String,
        #[facet(kdl::property)]
        #[facet(default)]
        timeout: Option<Spanned<u32>>,
    }

    #[derive(Debug)]
    struct TaskValidationError {
        src: String,
        issues: Vec<(Span, String)>,
    }

    impl fmt::Display for TaskValidationError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "task validation failed")
        }
    }

    impl std::error::Error for TaskValidationError {}

    impl Diagnostic for TaskValidationError {
        fn source_code(&self) -> Option<&dyn miette::SourceCode> {
            Some(&self.src)
        }

        fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
            Some(Box::new(self.issues.iter().map(|(span, msg)| {
                miette::LabeledSpan::at(span.offset..span.end(), msg.clone())
            })))
        }
    }

    let kdl_source = indoc! {r#"
        build timeout=0
        test timeout=999999
        deploy
    "#};

    let pipeline: Pipeline = facet_kdl_legacy::from_str(kdl_source).unwrap();

    // Validate all tasks and collect errors
    let mut issues = Vec::new();
    for task in &pipeline.tasks {
        if let Some(ref timeout) = task.timeout {
            if **timeout == 0 {
                issues.push((timeout.span(), "timeout cannot be zero".to_string()));
            } else if **timeout > 86400 {
                issues.push((timeout.span(), "timeout too large (max 86400s)".to_string()));
            }
        }
    }

    assert_eq!(issues.len(), 2); // Both build and test have invalid timeouts

    let err = TaskValidationError {
        src: kdl_source.to_string(),
        issues,
    };

    // Render with miette (with colors!)
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut output, &err).unwrap();

    println!("Multiple validation errors:\n{output}");

    // Should show both issues
    assert!(output.contains("timeout"));
}
