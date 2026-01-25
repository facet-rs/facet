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
