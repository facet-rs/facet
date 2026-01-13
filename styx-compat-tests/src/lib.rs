//! Cross-compatibility tests between facet-styx and serde_styx.
//!
//! These tests verify that:
//! 1. Both libraries produce identical output for the same data structures
//! 2. Output from one library can be parsed by the other
//! 3. Round-trips work across both libraries

#[cfg(test)]
mod tests {
    use facet::Facet;
    use serde::{Deserialize, Serialize};

    // ─────────────────────────────────────────────────────────────────────────
    // Test structures that implement both Facet and Serde traits
    // ─────────────────────────────────────────────────────────────────────────

    #[derive(Facet, Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct SimpleConfig {
        name: String,
        port: u16,
        debug: bool,
    }

    #[derive(Facet, Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct NestedConfig {
        server: ServerConfig,
        database: DatabaseConfig,
    }

    #[derive(Facet, Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct ServerConfig {
        host: String,
        port: u16,
    }

    #[derive(Facet, Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct DatabaseConfig {
        url: String,
        pool_size: u32,
    }

    #[derive(Facet, Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct WithSequence {
        name: String,
        values: Vec<i32>,
    }

    #[derive(Facet, Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct WithOptional {
        required: String,
        optional: Option<i32>,
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Identical output tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_identical_output_simple() {
        let value = SimpleConfig {
            name: "myapp".into(),
            port: 8080,
            debug: true,
        };

        let facet_output = facet_styx::to_string(&value).unwrap();
        let serde_output = serde_styx::to_string(&value).unwrap();

        assert_eq!(
            facet_output, serde_output,
            "facet-styx and serde_styx should produce identical output"
        );
    }

    #[test]
    fn test_identical_output_nested() {
        let value = NestedConfig {
            server: ServerConfig {
                host: "localhost".into(),
                port: 8080,
            },
            database: DatabaseConfig {
                url: "postgres://localhost/db".into(),
                pool_size: 10,
            },
        };

        let facet_output = facet_styx::to_string(&value).unwrap();
        let serde_output = serde_styx::to_string(&value).unwrap();

        assert_eq!(
            facet_output, serde_output,
            "facet-styx and serde_styx should produce identical output for nested structs"
        );
    }

    #[test]
    fn test_identical_output_compact() {
        let value = SimpleConfig {
            name: "test".into(),
            port: 3000,
            debug: false,
        };

        let facet_output = facet_styx::to_string_compact(&value).unwrap();
        let serde_output = serde_styx::to_string_compact(&value).unwrap();

        assert_eq!(
            facet_output, serde_output,
            "compact output should be identical"
        );
    }

    #[test]
    fn test_identical_output_sequence() {
        let value = WithSequence {
            name: "numbers".into(),
            values: vec![1, 2, 3, 4, 5],
        };

        let facet_output = facet_styx::to_string(&value).unwrap();
        let serde_output = serde_styx::to_string(&value).unwrap();

        assert_eq!(
            facet_output, serde_output,
            "sequence output should be identical"
        );
    }

    #[test]
    fn test_identical_output_optional_some() {
        let value = WithOptional {
            required: "hello".into(),
            optional: Some(42),
        };

        let facet_output = facet_styx::to_string(&value).unwrap();
        let serde_output = serde_styx::to_string(&value).unwrap();

        assert_eq!(
            facet_output, serde_output,
            "optional Some output should be identical"
        );
    }

    #[test]
    fn test_identical_output_optional_none() {
        let value = WithOptional {
            required: "hello".into(),
            optional: None,
        };

        let facet_output = facet_styx::to_string(&value).unwrap();
        let serde_output = serde_styx::to_string(&value).unwrap();

        assert_eq!(
            facet_output, serde_output,
            "optional None output should be identical"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Cross-library parsing tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_facet_output_parsed_by_serde() {
        let original = SimpleConfig {
            name: "crosstest".into(),
            port: 9000,
            debug: true,
        };

        // Serialize with facet-styx
        let facet_output = facet_styx::to_string(&original).unwrap();

        // Parse with serde_styx
        let parsed: SimpleConfig = serde_styx::from_str(&facet_output).unwrap();

        assert_eq!(
            original, parsed,
            "serde should parse facet output correctly"
        );
    }

    #[test]
    fn test_serde_output_parsed_by_facet() {
        let original = SimpleConfig {
            name: "crosstest".into(),
            port: 9000,
            debug: true,
        };

        // Serialize with serde_styx
        let serde_output = serde_styx::to_string(&original).unwrap();

        // Parse with facet-styx
        let parsed: SimpleConfig = facet_styx::from_str(&serde_output).unwrap();

        assert_eq!(
            original, parsed,
            "facet should parse serde output correctly"
        );
    }

    #[test]
    fn test_cross_parsing_nested() {
        let original = NestedConfig {
            server: ServerConfig {
                host: "127.0.0.1".into(),
                port: 443,
            },
            database: DatabaseConfig {
                url: "mysql://root@localhost/test".into(),
                pool_size: 5,
            },
        };

        // facet → serde
        let facet_output = facet_styx::to_string(&original).unwrap();
        let parsed_by_serde: NestedConfig = serde_styx::from_str(&facet_output).unwrap();
        assert_eq!(original, parsed_by_serde);

        // serde → facet
        let serde_output = serde_styx::to_string(&original).unwrap();
        let parsed_by_facet: NestedConfig = facet_styx::from_str(&serde_output).unwrap();
        assert_eq!(original, parsed_by_facet);
    }

    #[test]
    fn test_cross_parsing_sequence() {
        let original = WithSequence {
            name: "data".into(),
            values: vec![10, 20, 30, 40, 50],
        };

        // facet → serde
        let facet_output = facet_styx::to_string(&original).unwrap();
        let parsed_by_serde: WithSequence = serde_styx::from_str(&facet_output).unwrap();
        assert_eq!(original, parsed_by_serde);

        // serde → facet
        let serde_output = serde_styx::to_string(&original).unwrap();
        let parsed_by_facet: WithSequence = facet_styx::from_str(&serde_output).unwrap();
        assert_eq!(original, parsed_by_facet);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Round-trip tests across libraries
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_roundtrip_facet_serde_facet() {
        let original = SimpleConfig {
            name: "roundtrip".into(),
            port: 1234,
            debug: false,
        };

        // facet serialize → serde parse → facet serialize
        let step1 = facet_styx::to_string(&original).unwrap();
        let step2: SimpleConfig = serde_styx::from_str(&step1).unwrap();
        let step3 = facet_styx::to_string(&step2).unwrap();

        assert_eq!(step1, step3, "round-trip should produce identical output");
        assert_eq!(original, step2, "data should be preserved");
    }

    #[test]
    fn test_roundtrip_serde_facet_serde() {
        let original = SimpleConfig {
            name: "roundtrip".into(),
            port: 5678,
            debug: true,
        };

        // serde serialize → facet parse → serde serialize
        let step1 = serde_styx::to_string(&original).unwrap();
        let step2: SimpleConfig = facet_styx::from_str(&step1).unwrap();
        let step3 = serde_styx::to_string(&step2).unwrap();

        assert_eq!(step1, step3, "round-trip should produce identical output");
        assert_eq!(original, step2, "data should be preserved");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge case tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_quoted_strings_cross_compat() {
        let original = SimpleConfig {
            name: "hello world with spaces".into(),
            port: 80,
            debug: true,
        };

        let facet_output = facet_styx::to_string(&original).unwrap();
        let serde_output = serde_styx::to_string(&original).unwrap();

        assert_eq!(facet_output, serde_output);

        // Cross-parse
        let parsed_by_serde: SimpleConfig = serde_styx::from_str(&facet_output).unwrap();
        let parsed_by_facet: SimpleConfig = facet_styx::from_str(&serde_output).unwrap();

        assert_eq!(original, parsed_by_serde);
        assert_eq!(original, parsed_by_facet);
    }

    #[test]
    fn test_special_chars_cross_compat() {
        let original = SimpleConfig {
            name: "test{with}special(chars)".into(),
            port: 443,
            debug: false,
        };

        let facet_output = facet_styx::to_string(&original).unwrap();
        let serde_output = serde_styx::to_string(&original).unwrap();

        assert_eq!(facet_output, serde_output);

        // Cross-parse
        let parsed_by_serde: SimpleConfig = serde_styx::from_str(&facet_output).unwrap();
        let parsed_by_facet: SimpleConfig = facet_styx::from_str(&serde_output).unwrap();

        assert_eq!(original, parsed_by_serde);
        assert_eq!(original, parsed_by_facet);
    }
}
