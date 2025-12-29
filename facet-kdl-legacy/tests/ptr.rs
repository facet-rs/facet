// Allow box_collection in tests since we're specifically testing Box<String> handling
#![allow(clippy::box_collection)]

use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

// ============================================================================
// Pointer type support (Box<T>, Arc<T>, Rc<T>)
// ============================================================================

#[test]
fn box_scalar_value() {
    use std::boxed::Box;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        setting: Setting,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Setting {
        #[facet(kdl::argument)]
        value: Box<u32>,
    }

    let kdl = indoc! {r#"
        setting 42
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(*config.setting.value, 42);
}

#[test]
fn box_string_value() {
    use std::boxed::Box;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        message: Message,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Message {
        #[facet(kdl::argument)]
        text: Box<String>,
    }

    let kdl = indoc! {r#"
        message "Hello, World!"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(&*config.message.text, "Hello, World!");
}

#[test]
fn box_struct_child() {
    use std::boxed::Box;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Box<Server>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    let kdl = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 8080);
}

#[test]
fn arc_scalar_value() {
    use std::sync::Arc;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::child)]
        setting: Setting,
    }

    #[derive(Facet, Debug)]
    struct Setting {
        #[facet(kdl::argument)]
        value: Arc<u64>,
    }

    let kdl = indoc! {r#"
        setting 12345
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(*config.setting.value, 12345);
}

#[test]
fn arc_struct_child() {
    use std::sync::Arc;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        database: Arc<Database>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Database {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::property)]
        max_connections: u32,
    }

    let kdl = indoc! {r#"
        database "mydb" max_connections=100
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.database.name, "mydb");
    assert_eq!(config.database.max_connections, 100);
}

#[test]
fn rc_scalar_value() {
    use std::rc::Rc;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::child)]
        setting: Setting,
    }

    #[derive(Facet, Debug)]
    struct Setting {
        #[facet(kdl::argument)]
        value: Rc<i32>,
    }

    let kdl = indoc! {r#"
        setting -42
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(*config.setting.value, -42);
}

#[test]
fn option_box_combination() {
    use std::boxed::Box;

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
        description: Option<Box<String>>,
    }

    // With the optional boxed value
    let kdl = indoc! {r#"
        server "main" description="Primary server"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.name, "main");
    assert_eq!(
        config.server.description.as_deref(),
        Some(&"Primary server".to_string())
    );

    // Without the optional boxed value
    let kdl = indoc! {r#"
        server "backup"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.name, "backup");
    assert!(config.server.description.is_none());
}

#[test]
fn box_in_children_list() {
    use std::boxed::Box;

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::children)]
        items: Vec<Item>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        #[facet(kdl::argument)]
        value: Box<String>,
    }

    let kdl = indoc! {r#"
        item "first"
        item "second"
        item "third"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.items.len(), 3);
    assert_eq!(&*config.items[0].value, "first");
    assert_eq!(&*config.items[1].value, "second");
    assert_eq!(&*config.items[2].value, "third");
}
