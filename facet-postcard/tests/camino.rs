#![cfg(feature = "camino")]

use camino::Utf8PathBuf;
use facet::Facet;
use facet_postcard::{from_slice, to_vec};

#[test]
fn test_utf8pathbuf_basic() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        path: Utf8PathBuf,
    }

    let original = WithPath {
        path: Utf8PathBuf::from("/usr/local/bin"),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_empty() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        path: Utf8PathBuf,
    }

    let original = WithPath {
        path: Utf8PathBuf::from(""),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_relative() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        path: Utf8PathBuf,
    }

    let original = WithPath {
        path: Utf8PathBuf::from("src/main.rs"),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_windows_style() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        path: Utf8PathBuf,
    }

    let original = WithPath {
        path: Utf8PathBuf::from(r"C:\Users\example\Documents"),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_with_unicode() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        path: Utf8PathBuf,
    }

    let original = WithPath {
        path: Utf8PathBuf::from("/home/用户/文档"),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptionalPath {
        path: Option<Utf8PathBuf>,
    }

    // Test Some variant
    let original = WithOptionalPath {
        path: Some(Utf8PathBuf::from("/etc/config")),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptionalPath = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test None variant
    let original = WithOptionalPath { path: None };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptionalPath = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPaths {
        paths: Vec<Utf8PathBuf>,
    }

    let original = WithPaths {
        paths: vec![
            Utf8PathBuf::from("/usr/bin"),
            Utf8PathBuf::from("/usr/local/bin"),
            Utf8PathBuf::from("/opt/bin"),
        ],
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithPaths = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_nested() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Config {
        root: Utf8PathBuf,
        cache: Utf8PathBuf,
        temp: Utf8PathBuf,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Application {
        name: String,
        config: Config,
    }

    let original = Application {
        name: "MyApp".to_string(),
        config: Config {
            root: Utf8PathBuf::from("/app"),
            cache: Utf8PathBuf::from("/app/cache"),
            temp: Utf8PathBuf::from("/tmp/myapp"),
        },
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Application = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_in_enum() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    #[repr(C)]
    enum Location {
        Local(Utf8PathBuf),
        Remote { url: String },
    }

    // Test tuple variant
    let original = Location::Local(Utf8PathBuf::from("/local/path"));
    let bytes = to_vec(&original).unwrap();
    let decoded: Location = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test struct variant
    let original = Location::Remote {
        url: "https://example.com".to_string(),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: Location = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_in_box() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithBoxedPath {
        path: Box<Utf8PathBuf>,
    }

    let original = WithBoxedPath {
        path: Box::new(Utf8PathBuf::from("/boxed/path")),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: WithBoxedPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_multiple_fields() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct MultiPath {
        source: Utf8PathBuf,
        dest: Utf8PathBuf,
        backup: Option<Utf8PathBuf>,
        count: u32,
    }

    let original = MultiPath {
        source: Utf8PathBuf::from("/source/file.txt"),
        dest: Utf8PathBuf::from("/dest/file.txt"),
        backup: Some(Utf8PathBuf::from("/backup/file.txt")),
        count: 42,
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: MultiPath = from_slice(&bytes).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_roundtrip_special_chars() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        path: Utf8PathBuf,
    }

    // Test with spaces
    let original = WithPath {
        path: Utf8PathBuf::from("/path with spaces/file name.txt"),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test with dots
    let original = WithPath {
        path: Utf8PathBuf::from("./relative/../path/./file"),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithPath = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bare_utf8pathbuf() {
    facet_testhelpers::setup();

    let original = Utf8PathBuf::from("/standalone/path");
    let bytes = to_vec(&original).unwrap();
    let decoded: Utf8PathBuf = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_utf8pathbuf_serialization_format() {
    facet_testhelpers::setup();

    // Verify that Utf8PathBuf is serialized the same way as String
    // This ensures compatibility with postcard's format
    #[derive(Facet, PartialEq, Debug)]
    struct WithString {
        value: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct WithPath {
        value: Utf8PathBuf,
    }

    let path_str = "/usr/local/bin";
    let with_string = WithString {
        value: path_str.to_string(),
    };
    let with_path = WithPath {
        value: Utf8PathBuf::from(path_str),
    };

    let string_bytes = to_vec(&with_string).unwrap();
    let path_bytes = to_vec(&with_path).unwrap();

    // The bytes should be identical since Utf8PathBuf is just a wrapper around String
    assert_eq!(string_bytes, path_bytes);
}
