use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

// ============================================================================
// KDL type annotation support for enum disambiguation
// ============================================================================

/// Test that KDL type annotations can disambiguate flattened enum variants
/// when the solver cannot determine the variant from properties alone.
#[test]
fn type_annotation_disambiguates_enum() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        source: Source,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Source {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        kind: SourceKind,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SourceKind {
        // Both variants have 'url' - ambiguous without type annotation!
        Http(HttpSource),
        Git(GitSource),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct HttpSource {
        #[facet(kdl::property)]
        url: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct GitSource {
        #[facet(kdl::property)]
        url: String,
        #[facet(kdl::property, default)]
        branch: Option<String>,
    }

    // With type annotation (Http), disambiguate to Http variant
    let kdl = indoc! {r#"
        (Http)source "download" url="https://example.com/file.zip"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with Http type annotation");
    match &config.source.kind {
        SourceKind::Http(http) => {
            assert_eq!(http.url, "https://example.com/file.zip");
        }
        _ => panic!("expected Http variant"),
    }

    // With type annotation (Git), disambiguate to Git variant
    let kdl = indoc! {r#"
        (Git)source "repo" url="https://github.com/example/repo"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with Git type annotation");
    match &config.source.kind {
        SourceKind::Git(git) => {
            assert_eq!(git.url, "https://github.com/example/repo");
            assert_eq!(git.branch, None);
        }
        _ => panic!("expected Git variant"),
    }

    // Git with branch explicitly set
    let kdl = indoc! {r#"
        (Git)source "repo" url="https://github.com/example/repo" branch="main"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse Git with branch");
    match &config.source.kind {
        SourceKind::Git(git) => {
            assert_eq!(git.url, "https://github.com/example/repo");
            assert_eq!(git.branch, Some("main".to_string()));
        }
        _ => panic!("expected Git variant"),
    }
}

/// Test type annotation with unit variants
#[test]
fn type_annotation_unit_variant() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        log: LogConfig,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LogConfig {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        output: LogOutput,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum LogOutput {
        Stdout,
        File(FileOutput),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FileOutput {
        #[facet(kdl::property)]
        path: String,
    }

    // Explicitly select Stdout variant via type annotation
    let kdl = indoc! {r#"
        (Stdout)log "console"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse Stdout via type annotation");
    match &config.log.output {
        LogOutput::Stdout => { /* expected */ }
        _ => panic!("expected Stdout variant"),
    }

    // Explicitly select File variant via type annotation
    let kdl = indoc! {r#"
        (File)log "app" path="/var/log/app.log"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse File via type annotation");
    match &config.log.output {
        LogOutput::File(f) => {
            assert_eq!(f.path, "/var/log/app.log");
        }
        _ => panic!("expected File variant"),
    }
}

/// Test that kebab-case type annotations work (converted to PascalCase)
#[test]
fn type_annotation_kebab_case() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        source: Source,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Source {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        kind: SourceKind,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SourceKind {
        HttpSource(HttpSourceData),
        GitSource(GitSourceData),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct HttpSourceData {
        #[facet(kdl::property)]
        url: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct GitSourceData {
        #[facet(kdl::property)]
        url: String,
        #[facet(kdl::property, default)]
        branch: Option<String>,
    }

    // kebab-case type annotation should match PascalCase variant name
    let kdl = indoc! {r#"
        (http-source)source "download" url="https://example.com/file.zip"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with kebab-case type annotation");
    match &config.source.kind {
        SourceKind::HttpSource(http) => {
            assert_eq!(http.url, "https://example.com/file.zip");
        }
        _ => panic!("expected HttpSource variant"),
    }

    // Also test git-source
    let kdl = indoc! {r#"
        (git-source)source "repo" url="https://github.com/example/repo" branch="main"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse git-source kebab-case annotation");
    match &config.source.kind {
        SourceKind::GitSource(git) => {
            assert_eq!(git.url, "https://github.com/example/repo");
            assert_eq!(git.branch, Some("main".to_string()));
        }
        _ => panic!("expected GitSource variant"),
    }
}
