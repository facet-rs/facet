use facet::Facet;
use facet_args as args;

#[test]
fn test_config_attribute_without_prefix() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::config)]
        settings: ServerConfig,
    }

    #[derive(Facet)]
    struct ServerConfig {
        port: u16,
    }

    let shape = Args::SHAPE;
    let fields = match &shape.ty {
        facet_core::Type::User(facet_core::UserType::Struct(s)) => &s.fields,
        _ => panic!("expected struct"),
    };

    let settings_field = &fields[0];
    assert_eq!(settings_field.name, "settings");
    assert!(settings_field.has_attr(Some("args"), "config"));
}

#[test]
fn test_config_attribute_with_prefix() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::config, args::env_prefix = "REEF")]
        settings: ServerConfig,
    }

    #[derive(Facet)]
    struct ServerConfig {
        port: u16,
    }

    let shape = Args::SHAPE;
    let fields = match &shape.ty {
        facet_core::Type::User(facet_core::UserType::Struct(s)) => &s.fields,
        _ => panic!("expected struct"),
    };

    let settings_field = &fields[0];
    assert_eq!(settings_field.name, "settings");
    assert!(settings_field.has_attr(Some("args"), "config"));

    // Check for env_prefix attribute
    let env_prefix_attr = settings_field.get_attr(Some("args"), "env_prefix");
    assert!(
        env_prefix_attr.is_some(),
        "env_prefix attribute should exist"
    );

    // Try to extract the actual value
    if let Some(attr) = env_prefix_attr {
        let parsed = attr.get_as::<facet_args::Attr>();

        if let Some(facet_args::Attr::EnvPrefix(prefix_opt)) = parsed {
            if let Some(prefix) = prefix_opt {
                assert_eq!(*prefix, "REEF");
            } else {
                panic!("env_prefix was Some(None), expected Some(Some('REEF'))");
            }
        } else {
            panic!("env_prefix should be EnvPrefix variant, got: {:?}", parsed);
        }
    }
}

#[test]
fn test_env_var_list_handling() {
    use facet_args::env::{EnvConfig, MockEnv, parse_env_with_source};

    // Test comma-separated values are parsed into arrays
    let env = MockEnv::from_pairs([
        (
            "TEST__EMAILS",
            "alice@example.com,bob@example.com,charlie@example.com",
        ),
        ("TEST__PORTS", "8080,8081,8082"),
    ]);

    let config = EnvConfig::new("TEST");
    let result = parse_env_with_source(&config, &env);

    use facet_args::config_value::ConfigValue;
    if let ConfigValue::Object(obj) = result.value {
        // Check emails array
        if let Some(ConfigValue::Array(emails)) = obj.value.get("emails") {
            assert_eq!(emails.value.len(), 3);
            if let ConfigValue::String(s) = &emails.value[0] {
                assert_eq!(s.value, "alice@example.com");
            } else {
                panic!("expected string in emails array");
            }
            if let ConfigValue::String(s) = &emails.value[1] {
                assert_eq!(s.value, "bob@example.com");
            } else {
                panic!("expected string in emails array");
            }
        } else {
            panic!("expected array for emails");
        }

        // Check ports array
        if let Some(ConfigValue::Array(ports)) = obj.value.get("ports") {
            assert_eq!(ports.value.len(), 3);
            if let ConfigValue::String(s) = &ports.value[0] {
                assert_eq!(s.value, "8080");
            } else {
                panic!("expected string in ports array");
            }
        } else {
            panic!("expected array for ports");
        }
    } else {
        panic!("expected object");
    }
}

#[test]
fn test_env_var_escaped_comma() {
    use facet_args::env::{EnvConfig, MockEnv, parse_env_with_source};

    // Test that escaped commas are handled as single string values
    let env = MockEnv::from_pairs([("TEST__DESCRIPTION", r"This value has\, a comma in it")]);

    let config = EnvConfig::new("TEST");
    let result = parse_env_with_source(&config, &env);

    use facet_args::config_value::ConfigValue;
    if let ConfigValue::Object(obj) = result.value {
        if let Some(ConfigValue::String(desc)) = obj.value.get("description") {
            assert_eq!(desc.value, "This value has, a comma in it");
        } else {
            panic!("expected string for description with escaped comma");
        }
    } else {
        panic!("expected object");
    }
}

#[test]
fn test_env_var_mixed_list_and_string() {
    use facet_args::env::{EnvConfig, MockEnv, parse_env_with_source};

    // Test that we can have both list and string values
    let env = MockEnv::from_pairs([
        ("TEST__ALLOWED_HOSTS", "localhost,127.0.0.1,::1"),
        ("TEST__DATABASE_URL", "postgres://localhost/mydb"),
        ("TEST__API_KEYS", "key1,key2,key3"),
    ]);

    let config = EnvConfig::new("TEST");
    let result = parse_env_with_source(&config, &env);

    use facet_args::config_value::ConfigValue;
    if let ConfigValue::Object(obj) = result.value {
        // allowed_hosts should be an array
        if let Some(ConfigValue::Array(hosts)) = obj.value.get("allowed_hosts") {
            assert_eq!(hosts.value.len(), 3);
        } else {
            panic!("expected array for allowed_hosts");
        }

        // database_url should be a string (no commas)
        if let Some(ConfigValue::String(url)) = obj.value.get("database_url") {
            assert_eq!(url.value, "postgres://localhost/mydb");
        } else {
            panic!("expected string for database_url");
        }

        // api_keys should be an array
        if let Some(ConfigValue::Array(keys)) = obj.value.get("api_keys") {
            assert_eq!(keys.value.len(), 3);
        } else {
            panic!("expected array for api_keys");
        }
    } else {
        panic!("expected object");
    }
}

#[test]
fn test_env_var_numeric_lists() {
    use facet_args::config_value_parser::from_config_value;
    use facet_args::env::{EnvConfig, MockEnv, parse_env_with_source};

    #[derive(Facet)]
    struct Config {
        ports: Vec<u16>,
        scores: Vec<i32>,
        ratios: Vec<f64>,
    }

    // Test that numeric lists are parsed correctly
    let env = MockEnv::from_pairs([
        ("TEST__PORTS", "8080,8081,8082"),
        ("TEST__SCORES", "-10,0,42,100"),
        ("TEST__RATIOS", "1.5,2.7,3.14"),
    ]);

    let config = EnvConfig::new("TEST");
    let result = parse_env_with_source(&config, &env);

    // The env vars come in as string arrays, but should coerce to numeric types
    let config: Config = from_config_value(&result.value).expect("should parse config");

    assert_eq!(config.ports, vec![8080, 8081, 8082]);
    assert_eq!(config.scores, vec![-10, 0, 42, 100]);
    assert_eq!(config.ratios.len(), 3);
    assert!((config.ratios[0] - 1.5).abs() < 0.01);
    assert!((config.ratios[1] - 2.7).abs() < 0.01);
    assert!((config.ratios[2] - std::f64::consts::PI).abs() < 0.01);
}
