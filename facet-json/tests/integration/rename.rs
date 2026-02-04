//! Tests for rename and rename_all attribute support in JSON serialization/deserialization.

#![allow(non_snake_case)]

use facet::Facet;
use facet_json::{from_str, to_vec};
use facet_testhelpers::test;

// =============================================================================
// Enum rename_all tests
// =============================================================================

#[test]
fn enum_rename_all_snake_case_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "snake_case")]
    enum ValidationErrorCode {
        CircularDependency,
        InvalidNaming,
        UnknownRequirement,
    }

    let json =
        String::from_utf8(to_vec(&ValidationErrorCode::CircularDependency).unwrap()).unwrap();
    assert_eq!(json, r#""circular_dependency""#);

    let json = String::from_utf8(to_vec(&ValidationErrorCode::InvalidNaming).unwrap()).unwrap();
    assert_eq!(json, r#""invalid_naming""#);

    let json =
        String::from_utf8(to_vec(&ValidationErrorCode::UnknownRequirement).unwrap()).unwrap();
    assert_eq!(json, r#""unknown_requirement""#);
}

#[test]
fn enum_rename_all_snake_case_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "snake_case")]
    enum ValidationErrorCode {
        CircularDependency,
        InvalidNaming,
        UnknownRequirement,
    }

    let result: ValidationErrorCode = from_str(r#""circular_dependency""#).unwrap();
    assert_eq!(result, ValidationErrorCode::CircularDependency);

    let result: ValidationErrorCode = from_str(r#""invalid_naming""#).unwrap();
    assert_eq!(result, ValidationErrorCode::InvalidNaming);

    let result: ValidationErrorCode = from_str(r#""unknown_requirement""#).unwrap();
    assert_eq!(result, ValidationErrorCode::UnknownRequirement);
}

#[test]
fn enum_rename_all_camel_case_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "camelCase")]
    enum HttpMethod {
        GetRequest,
        PostData,
        DeleteItem,
    }

    let json = String::from_utf8(to_vec(&HttpMethod::GetRequest).unwrap()).unwrap();
    assert_eq!(json, r#""getRequest""#);

    let json = String::from_utf8(to_vec(&HttpMethod::PostData).unwrap()).unwrap();
    assert_eq!(json, r#""postData""#);
}

#[test]
fn enum_rename_all_camel_case_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "camelCase")]
    enum HttpMethod {
        GetRequest,
        PostData,
        DeleteItem,
    }

    let result: HttpMethod = from_str(r#""getRequest""#).unwrap();
    assert_eq!(result, HttpMethod::GetRequest);

    let result: HttpMethod = from_str(r#""postData""#).unwrap();
    assert_eq!(result, HttpMethod::PostData);
}

// =============================================================================
// Enum individual rename tests
// =============================================================================

#[test]
fn enum_individual_rename_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum GitStatus {
        #[facet(rename = "dirty")]
        Dirty,
        #[facet(rename = "staged")]
        Staged,
        #[facet(rename = "clean")]
        Clean,
    }

    let json = String::from_utf8(to_vec(&GitStatus::Dirty).unwrap()).unwrap();
    assert_eq!(json, r#""dirty""#);

    let json = String::from_utf8(to_vec(&GitStatus::Staged).unwrap()).unwrap();
    assert_eq!(json, r#""staged""#);

    let json = String::from_utf8(to_vec(&GitStatus::Clean).unwrap()).unwrap();
    assert_eq!(json, r#""clean""#);
}

#[test]
fn enum_individual_rename_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum GitStatus {
        #[facet(rename = "dirty")]
        Dirty,
        #[facet(rename = "staged")]
        Staged,
        #[facet(rename = "clean")]
        Clean,
    }

    let result: GitStatus = from_str(r#""dirty""#).unwrap();
    assert_eq!(result, GitStatus::Dirty);

    let result: GitStatus = from_str(r#""staged""#).unwrap();
    assert_eq!(result, GitStatus::Staged);

    let result: GitStatus = from_str(r#""clean""#).unwrap();
    assert_eq!(result, GitStatus::Clean);
}

// =============================================================================
// Struct rename_all tests
// =============================================================================

#[test]
fn struct_rename_all_camel_case_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "camelCase")]
    struct ApiResponse {
        user_name: String,
        created_at: String,
        is_active: bool,
    }

    let response = ApiResponse {
        user_name: "alice".to_string(),
        created_at: "2024-01-01".to_string(),
        is_active: true,
    };

    let json = String::from_utf8(to_vec(&response).unwrap()).unwrap();
    assert_eq!(
        json,
        r#"{"userName":"alice","createdAt":"2024-01-01","isActive":true}"#
    );
}

#[test]
fn struct_rename_all_camel_case_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "camelCase")]
    struct ApiResponse {
        user_name: String,
        created_at: String,
        is_active: bool,
    }

    let result: ApiResponse =
        from_str(r#"{"userName":"alice","createdAt":"2024-01-01","isActive":true}"#).unwrap();

    assert_eq!(result.user_name, "alice");
    assert_eq!(result.created_at, "2024-01-01");
    assert!(result.is_active);
}

#[test]
fn struct_rename_all_snake_case_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "snake_case")]
    struct UserProfile {
        FirstName: String,
        LastName: String,
        EmailAddress: String,
    }

    let profile = UserProfile {
        FirstName: "John".to_string(),
        LastName: "Doe".to_string(),
        EmailAddress: "john@example.com".to_string(),
    };

    let json = String::from_utf8(to_vec(&profile).unwrap()).unwrap();
    assert_eq!(
        json,
        r#"{"first_name":"John","last_name":"Doe","email_address":"john@example.com"}"#
    );
}

#[test]
fn struct_rename_all_snake_case_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "snake_case")]
    struct UserProfile {
        FirstName: String,
        LastName: String,
        EmailAddress: String,
    }

    let result: UserProfile =
        from_str(r#"{"first_name":"John","last_name":"Doe","email_address":"john@example.com"}"#)
            .unwrap();

    assert_eq!(result.FirstName, "John");
    assert_eq!(result.LastName, "Doe");
    assert_eq!(result.EmailAddress, "john@example.com");
}

// =============================================================================
// Struct individual rename tests
// =============================================================================

#[test]
fn struct_individual_rename_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    struct UserData {
        #[facet(rename = "userName")]
        user_name: String,
        #[facet(rename = "emailAddress")]
        email: String,
    }

    let data = UserData {
        user_name: "bob".to_string(),
        email: "bob@example.com".to_string(),
    };

    let json = String::from_utf8(to_vec(&data).unwrap()).unwrap();
    assert_eq!(
        json,
        r#"{"userName":"bob","emailAddress":"bob@example.com"}"#
    );
}

#[test]
fn struct_individual_rename_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    struct UserData {
        #[facet(rename = "userName")]
        user_name: String,
        #[facet(rename = "emailAddress")]
        email: String,
    }

    let result: UserData =
        from_str(r#"{"userName":"bob","emailAddress":"bob@example.com"}"#).unwrap();

    assert_eq!(result.user_name, "bob");
    assert_eq!(result.email, "bob@example.com");
}

// =============================================================================
// Enum with data and rename_all tests
// =============================================================================

#[test]
fn enum_with_struct_data_rename_all_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(rename_all = "snake_case")]
    enum Message {
        TextMessage { content: String },
        ImageUpload { url: String, width: u32 },
    }

    let msg = Message::TextMessage {
        content: "hello".to_string(),
    };
    let json = String::from_utf8(to_vec(&msg).unwrap()).unwrap();
    assert_eq!(json, r#"{"text_message":{"content":"hello"}}"#);

    let msg = Message::ImageUpload {
        url: "http://example.com/img.png".to_string(),
        width: 800,
    };
    let json = String::from_utf8(to_vec(&msg).unwrap()).unwrap();
    assert_eq!(
        json,
        r#"{"image_upload":{"url":"http://example.com/img.png","width":800}}"#
    );
}

#[test]
fn enum_with_struct_data_rename_all_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(rename_all = "snake_case")]
    enum Message {
        TextMessage { content: String },
        ImageUpload { url: String, width: u32 },
    }

    let result: Message = from_str(r#"{"text_message":{"content":"hello"}}"#).unwrap();
    assert_eq!(
        result,
        Message::TextMessage {
            content: "hello".to_string()
        }
    );

    let result: Message =
        from_str(r#"{"image_upload":{"url":"http://example.com/img.png","width":800}}"#).unwrap();
    assert_eq!(
        result,
        Message::ImageUpload {
            url: "http://example.com/img.png".to_string(),
            width: 800
        }
    );
}

#[test]
fn enum_with_newtype_data_rename_all_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "snake_case")]
    enum Wrapper {
        StringValue(String),
        NumberValue(i32),
    }

    let val = Wrapper::StringValue("test".to_string());
    let json = String::from_utf8(to_vec(&val).unwrap()).unwrap();
    assert_eq!(json, r#"{"string_value":"test"}"#);

    let val = Wrapper::NumberValue(42);
    let json = String::from_utf8(to_vec(&val).unwrap()).unwrap();
    assert_eq!(json, r#"{"number_value":42}"#);
}

#[test]
fn enum_with_newtype_data_rename_all_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "snake_case")]
    enum Wrapper {
        StringValue(String),
        NumberValue(i32),
    }

    let result: Wrapper = from_str(r#"{"string_value":"test"}"#).unwrap();
    assert_eq!(result, Wrapper::StringValue("test".to_string()));

    let result: Wrapper = from_str(r#"{"number_value":42}"#).unwrap();
    assert_eq!(result, Wrapper::NumberValue(42));
}

// =============================================================================
// Internally tagged enum with rename_all tests
// =============================================================================

#[test]
fn internally_tagged_enum_rename_all_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "type", rename_all = "snake_case")]
    enum Event {
        UserCreated { user_id: u64 },
        UserDeleted { user_id: u64 },
    }

    let event = Event::UserCreated { user_id: 123 };
    let json = String::from_utf8(to_vec(&event).unwrap()).unwrap();
    assert_eq!(json, r#"{"type":"user_created","user_id":123}"#);

    let event = Event::UserDeleted { user_id: 456 };
    let json = String::from_utf8(to_vec(&event).unwrap()).unwrap();
    assert_eq!(json, r#"{"type":"user_deleted","user_id":456}"#);
}

#[test]
fn internally_tagged_enum_rename_all_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "type", rename_all = "snake_case")]
    enum Event {
        UserCreated { user_id: u64 },
        UserDeleted { user_id: u64 },
    }

    let result: Event = from_str(r#"{"type":"user_created","user_id":123}"#).unwrap();
    assert_eq!(result, Event::UserCreated { user_id: 123 });

    let result: Event = from_str(r#"{"type":"user_deleted","user_id":456}"#).unwrap();
    assert_eq!(result, Event::UserDeleted { user_id: 456 });
}

// =============================================================================
// Adjacently tagged enum with rename_all tests
// =============================================================================

#[test]
fn adjacently_tagged_enum_rename_all_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "kind", content = "data", rename_all = "snake_case")]
    enum Action {
        CreateUser { name: String },
        DeleteUser { id: u64 },
    }

    let action = Action::CreateUser {
        name: "alice".to_string(),
    };
    let json = String::from_utf8(to_vec(&action).unwrap()).unwrap();
    assert_eq!(json, r#"{"kind":"create_user","data":{"name":"alice"}}"#);
}

#[test]
fn adjacently_tagged_enum_rename_all_deserialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "kind", content = "data", rename_all = "snake_case")]
    enum Action {
        CreateUser { name: String },
        DeleteUser { id: u64 },
    }

    let result: Action = from_str(r#"{"kind":"create_user","data":{"name":"alice"}}"#).unwrap();
    assert_eq!(
        result,
        Action::CreateUser {
            name: "alice".to_string()
        }
    );

    let result: Action = from_str(r#"{"kind":"delete_user","data":{"id":123}}"#).unwrap();
    assert_eq!(result, Action::DeleteUser { id: 123 });
}

// =============================================================================
// Round-trip tests
// =============================================================================

#[test]
fn enum_rename_all_round_trip() {
    #[derive(Debug, Facet, PartialEq, Clone)]
    #[repr(u8)]
    #[facet(rename_all = "snake_case")]
    enum Status {
        InProgress,
        CompletedSuccessfully,
        FailedWithError,
    }

    for status in [
        Status::InProgress,
        Status::CompletedSuccessfully,
        Status::FailedWithError,
    ] {
        let json = String::from_utf8(to_vec(&status).unwrap()).unwrap();
        let parsed: Status = from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }
}

#[test]
fn struct_rename_all_round_trip() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "camelCase")]
    struct Config {
        max_connections: u32,
        timeout_seconds: u64,
        enable_logging: bool,
    }

    let config = Config {
        max_connections: 100,
        timeout_seconds: 30,
        enable_logging: true,
    };

    let json = String::from_utf8(to_vec(&config).unwrap()).unwrap();
    let parsed: Config = from_str(&json).unwrap();
    assert_eq!(parsed, config);
}
