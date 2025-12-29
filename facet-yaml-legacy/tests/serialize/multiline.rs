//! Tests for multi-line string serialization using YAML block scalars.

use eyre::Result;
use facet::Facet;

/// Test that multi-line strings use block scalar syntax (|).
#[test]
fn test_multiline_string_uses_block_scalar() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        script: String,
    }

    let script = "set -e\necho hello\necho world";
    let config = Config {
        script: script.to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&config)?;

    // Should use block scalar syntax, not escaped newlines
    assert!(
        yaml.contains("|"),
        "Expected block scalar syntax (|), got:\n{yaml}"
    );
    assert!(
        !yaml.contains("\\n"),
        "Should not contain escaped newlines, got:\n{yaml}"
    );

    // Verify round-trip works
    let deserialized: Config = facet_yaml_legacy::from_str(&yaml)
        .map_err(|e| eyre::eyre!("{e}"))
        .map_err(|e| e.wrap_err(format!("Failed to parse:\n{yaml}")))?;
    assert_eq!(deserialized.script, script);

    Ok(())
}

/// Test block scalar with trailing newline (clip chomping, default).
#[test]
fn test_multiline_string_with_trailing_newline() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        script: String,
    }

    let script = "line1\nline2\n";
    let config = Config {
        script: script.to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&config)?;
    assert!(yaml.contains("|"), "Expected block scalar, got:\n{yaml}");

    // Round-trip should preserve the trailing newline
    let deserialized: Config = facet_yaml_legacy::from_str(&yaml).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(deserialized.script, script);

    Ok(())
}

/// Test block scalar without trailing newline (strip chomping: |-).
#[test]
fn test_multiline_string_without_trailing_newline() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        script: String,
    }

    let script = "line1\nline2";
    let config = Config {
        script: script.to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&config)?;
    assert!(
        yaml.contains("|-"),
        "Expected strip chomping (|-), got:\n{yaml}"
    );

    // Round-trip should preserve no trailing newline
    let deserialized: Config = facet_yaml_legacy::from_str(&yaml).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(deserialized.script, script);

    Ok(())
}

/// Test block scalar with multiple trailing newlines (keep chomping: |+).
///
/// Note: to_string strips one trailing newline from the document for consistency.
/// For block scalars with |+, the to_writer output correctly preserves all trailing
/// newlines. When using to_string, round-trip may lose one trailing newline if the
/// block scalar is at the end of the document. Use to_writer for exact preservation.
#[test]
fn test_multiline_string_with_multiple_trailing_newlines() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        script: String,
    }

    let script = "line1\nline2\n\n";
    let config = Config {
        script: script.to_string(),
    };

    // to_writer produces correct YAML with proper trailing newlines
    let mut raw_output = Vec::new();
    facet_yaml_legacy::to_writer(&mut raw_output, &config)?;
    let raw_yaml = String::from_utf8(raw_output).unwrap();
    assert!(
        raw_yaml.contains("|+"),
        "Expected keep chomping (|+), got:\n{raw_yaml}"
    );

    // Verify round-trip with to_writer preserves all trailing newlines
    let deserialized: Config = facet_yaml_legacy::from_str(&raw_yaml).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(
        deserialized.script, script,
        "to_writer round-trip should preserve all trailing newlines"
    );

    // to_string strips one trailing newline from the document, which affects |+ scalars
    // at document end. This is a known limitation.
    let yaml = facet_yaml_legacy::to_string(&config)?;
    assert!(
        yaml.contains("|+"),
        "Expected keep chomping (|+), got:\n{yaml}"
    );

    Ok(())
}

/// Test that single-line strings don't use block scalars.
#[test]
fn test_single_line_string_no_block_scalar() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
    }

    let config = Config {
        name: "simple".to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&config)?;
    assert!(
        !yaml.contains("|"),
        "Single-line string should not use block scalar, got:\n{yaml}"
    );

    Ok(())
}

/// Test that strings with \r (carriage return) fall back to quoted format.
#[test]
fn test_carriage_return_uses_quoted() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        data: String,
    }

    let config = Config {
        data: "line1\r\nline2".to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&config)?;
    // Should use quoted format for Windows line endings
    assert!(
        yaml.contains("\""),
        "Windows line endings should use quoted format, got:\n{yaml}"
    );

    Ok(())
}

/// Test realistic GitHub Actions workflow script.
#[test]
fn test_github_actions_script() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Step {
        run: String,
    }

    let script = r#"set -e
case "$GITHUB_REF" in
  refs/tags/v*)
    VERSION="${GITHUB_REF#refs/tags/v}"
    IS_RELEASE="true"
    ;;
  *)
    VERSION="0.0.0-dev"
    IS_RELEASE="false"
    ;;
esac
echo "version=$VERSION" >> $GITHUB_OUTPUT
echo "is_release=$IS_RELEASE" >> $GITHUB_OUTPUT
echo "Version: $VERSION (release: $IS_RELEASE)""#;

    let step = Step {
        run: script.to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&step)?;

    // Should be much more readable than escaped format
    assert!(
        yaml.contains("|"),
        "Expected block scalar for multi-line script, got:\n{yaml}"
    );
    assert!(
        !yaml.contains("\\n"),
        "Should not contain escaped newlines, got:\n{yaml}"
    );

    // The output should contain the readable script content
    assert!(
        yaml.contains("set -e"),
        "Block scalar should contain readable script, got:\n{yaml}"
    );
    assert!(
        yaml.contains("GITHUB_REF"),
        "Block scalar should preserve shell variables, got:\n{yaml}"
    );

    // Verify round-trip
    let deserialized: Step = facet_yaml_legacy::from_str(&yaml).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(deserialized.run, script);

    Ok(())
}

/// Test whitespace-only strings with newlines use quoted format.
#[test]
fn test_whitespace_only_uses_quoted() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        data: String,
    }

    let config = Config {
        data: "   \n   \n   ".to_string(),
    };

    let yaml = facet_yaml_legacy::to_string(&config)?;
    // Whitespace-only multi-line strings should use quoted format
    assert!(
        yaml.contains("\""),
        "Whitespace-only should use quoted format, got:\n{yaml}"
    );

    Ok(())
}

/// Test nested struct with multi-line string.
#[test]
fn test_nested_multiline_string() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Inner {
        content: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    let outer = Outer {
        name: "test".to_string(),
        inner: Inner {
            content: "line1\nline2\nline3".to_string(),
        },
    };

    let yaml = facet_yaml_legacy::to_string(&outer)?;

    assert!(
        yaml.contains("|-"),
        "Nested multi-line string should use block scalar, got:\n{yaml}"
    );

    let deserialized: Outer = facet_yaml_legacy::from_str(&yaml).map_err(|e| eyre::eyre!("{e}"))?;
    assert_eq!(deserialized, outer);

    Ok(())
}
