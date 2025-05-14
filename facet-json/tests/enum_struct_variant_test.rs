use eyre::Result;
use facet::Facet;
use facet_testhelpers::setup;

/**
 * This test verifies that Facet can properly serialize and deserialize
 * enum variants that contain structs.
 */

#[test]
fn enum_struct_variants() -> Result<()> {
    setup();

    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    enum Message {
        Good { greeting: String, time: i32 },
        Tenant { id: String, action: String },
    }

    // Test struct variant serialization
    let good = Message::Good {
        greeting: "Hello, sunshine!".to_string(),
        time: 800,
    };

    assert_eq!(
        facet_json::to_string(&good),
        r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#
    );

    let tenant = Message::Tenant {
        id: "tenant-123".to_string(),
        action: "login".to_string(),
    };

    assert_eq!(
        facet_json::to_string(&tenant),
        r#"{"Tenant":{"id":"tenant-123","action":"login"}}"#
    );

    // Test struct variant deserialization
    let json_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    let deserialized_good: Message =
        facet_json::from_str(json_good).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_good, good);

    let json_tenant = r#"{"Tenant":{"id":"tenant-123","action":"login"}}"#;
    let deserialized_tenant: Message =
        facet_json::from_str(json_tenant).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_tenant, tenant);

    // Test roundtrip
    let json = facet_json::to_string(&good);
    let roundtrip: Message = facet_json::from_str(&json).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(roundtrip, good);

    Ok(())
}

#[test]
fn tuple_struct_variants() -> Result<()> {
    setup();

    #[derive(Debug, Facet, PartialEq)]
    struct GoodMorning {
        greeting: String,
        time: i32,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct TenantEvent {
        tenant_id: String,
        action: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum MomEvent {
        Good(GoodMorning) = 1,
        Tenant(TenantEvent) = 2,
    }

    // Test serialization
    let good = MomEvent::Good(GoodMorning {
        greeting: "Hello, sunshine!".to_string(),
        time: 800,
    });

    // NOTE: The expected JSON is serialized with the variant name and the struct fields
    let expected_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    assert_eq!(facet_json::to_string(&good), expected_good);

    let tenant = MomEvent::Tenant(TenantEvent {
        tenant_id: "tenant-123".to_string(),
        action: "login".to_string(),
    });

    let expected_tenant = r#"{"Tenant":{"tenant_id":"tenant-123","action":"login"}}"#;
    assert_eq!(facet_json::to_string(&tenant), expected_tenant);

    // Test deserialization
    let json_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    let deserialized_good: MomEvent =
        facet_json::from_str(json_good).map_err(|e| eyre::eyre!("{}", e))?;

    match deserialized_good {
        MomEvent::Good(gm) => {
            assert_eq!(gm.greeting, "Hello, sunshine!");
            assert_eq!(gm.time, 800);
        }
        _ => panic!("Expected Good variant"),
    }

    let json_tenant = r#"{"Tenant":{"tenant_id":"tenant-123","action":"login"}}"#;
    let deserialized_tenant: MomEvent =
        facet_json::from_str(json_tenant).map_err(|e| eyre::eyre!("{}", e))?;

    match deserialized_tenant {
        MomEvent::Tenant(te) => {
            assert_eq!(te.tenant_id, "tenant-123");
            assert_eq!(te.action, "login");
        }
        _ => panic!("Expected Tenant variant"),
    }

    Ok(())
}
