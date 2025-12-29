use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

// ============================================================================
// Transparent/inner type support (like Utf8PathBuf, newtypes)
// ============================================================================

#[test]
fn transparent_utf8_path_buf() {
    use camino::Utf8PathBuf;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        file: FileConfig,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FileConfig {
        #[facet(kdl::argument)]
        path: Utf8PathBuf,
    }

    let kdl = indoc! {r#"
        file "/home/user/config.kdl"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.file.path.as_str(), "/home/user/config.kdl");
}

#[test]
fn transparent_utf8_path_buf_property() {
    use camino::Utf8PathBuf;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::property)]
        config_path: Utf8PathBuf,
        #[facet(kdl::property)]
        log_path: Utf8PathBuf,
    }

    let kdl = indoc! {r#"
        server "main" config_path="/etc/app/config.kdl" log_path="/var/log/app.log"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.name, "main");
    assert_eq!(config.server.config_path.as_str(), "/etc/app/config.kdl");
    assert_eq!(config.server.log_path.as_str(), "/var/log/app.log");
}

#[test]
fn transparent_option_utf8_path_buf() {
    use camino::Utf8PathBuf;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::property, default)]
        config_path: Option<Utf8PathBuf>,
    }

    // With path
    let kdl = indoc! {r#"
        server "main" config_path="/etc/app/config.kdl"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.name, "main");
    assert_eq!(
        config.server.config_path.as_ref().map(|p| p.as_str()),
        Some("/etc/app/config.kdl")
    );

    // Without path
    let kdl = indoc! {r#"
        server "backup"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.name, "backup");
    assert!(config.server.config_path.is_none());
}

// ============================================================================
// Transparent map keys support
// ============================================================================

/// Test map with transparent key type (Utf8PathBuf as key)
#[test]
fn map_with_transparent_key_utf8_path_buf() {
    use camino::Utf8PathBuf;
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        files: HashMap<Utf8PathBuf, FileConfig>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FileConfig {
        #[facet(kdl::property)]
        enabled: bool,
    }

    // KDL node names (using valid KDL identifier syntax)
    // Note: KDL v2 requires #true/#false for boolean values
    let kdl = indoc! {r#"
        config_file enabled=#true
        log_file enabled=#false
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.files.len(), 2);

    let config_file = config
        .files
        .get(&Utf8PathBuf::from("config_file"))
        .expect("should have config file");
    assert!(config_file.enabled);

    let log_file = config
        .files
        .get(&Utf8PathBuf::from("log_file"))
        .expect("should have log file");
    assert!(!log_file.enabled);
}

/// Test BTreeMap with transparent key type
#[test]
fn btreemap_with_transparent_key_utf8_path_buf() {
    use camino::Utf8PathBuf;
    use std::collections::BTreeMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        paths: BTreeMap<Utf8PathBuf, u32>,
    }

    // KDL node names (using valid KDL identifier syntax)
    let kdl = indoc! {r#"
        z_last 3
        a_first 1
        m_middle 2
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.paths.len(), 3);

    // BTreeMap with Utf8PathBuf keys should iterate in sorted order
    let keys: Vec<_> = config.paths.keys().map(|p| p.as_str()).collect();
    assert_eq!(keys, vec!["a_first", "m_middle", "z_last"]);

    assert_eq!(config.paths.get(&Utf8PathBuf::from("a_first")), Some(&1));
    assert_eq!(config.paths.get(&Utf8PathBuf::from("m_middle")), Some(&2));
    assert_eq!(config.paths.get(&Utf8PathBuf::from("z_last")), Some(&3));
}
