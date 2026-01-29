use facet::Facet;
use facet_json::{from_str, to_string};

/// Test nested internal tagging - an outer enum tagged by one field,
/// containing an inner enum tagged by another field.
///
/// This models the nextest libtest-json-plus output format:
/// - Outer tag: "type" = "suite" | "test"
/// - Inner tag: "event" = "started" | "ok" | "failed"

#[derive(Debug, Facet, PartialEq)]
struct NextestMeta {
    #[facet(rename = "crate")]
    krate: String,
    test_binary: String,
    kind: String,
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
#[facet(tag = "event", rename_all = "snake_case")]
enum SuiteEvent {
    Started {
        test_count: u32,
        nextest: NextestMeta,
    },
    Failed {
        passed: u32,
        failed: u32,
        ignored: u32,
        exec_time: f64,
        nextest: NextestMeta,
    },
    Ok {
        passed: u32,
        failed: u32,
        ignored: u32,
        exec_time: f64,
        nextest: NextestMeta,
    },
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
#[facet(tag = "event", rename_all = "snake_case")]
enum TestEvent {
    Started {
        name: String,
    },
    Ok {
        name: String,
        exec_time: f64,
    },
    Failed {
        name: String,
        exec_time: f64,
        stdout: String,
    },
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
#[facet(tag = "type", rename_all = "snake_case")]
enum NextestMessage {
    Suite(SuiteEvent),
    Test(TestEvent),
}

#[test]
fn nested_internal_tagging_roundtrip_suite_started() {
    let msg = NextestMessage::Suite(SuiteEvent::Started {
        test_count: 6,
        nextest: NextestMeta {
            krate: "sample-crate".to_string(),
            test_binary: "sample_crate".to_string(),
            kind: "lib".to_string(),
        },
    });

    let json = to_string(&msg).unwrap();
    eprintln!("Serialized: {json}");

    // Should have both tags
    assert!(json.contains(r#""type":"suite""#), "missing type tag");
    assert!(json.contains(r#""event":"started""#), "missing event tag");

    let parsed: NextestMessage = from_str(&json).unwrap();
    assert_eq!(parsed, msg);
}

#[test]
fn nested_internal_tagging_roundtrip_test_failed() {
    let msg = NextestMessage::Test(TestEvent::Failed {
        name: "sample-crate::tests::test_panic".to_string(),
        exec_time: 0.005,
        stdout: "thread panicked at src/lib.rs:10:5".to_string(),
    });

    let json = to_string(&msg).unwrap();
    eprintln!("Serialized: {json}");

    assert!(json.contains(r#""type":"test""#), "missing type tag");
    assert!(json.contains(r#""event":"failed""#), "missing event tag");

    let parsed: NextestMessage = from_str(&json).unwrap();
    assert_eq!(parsed, msg);
}

#[test]
fn nested_internal_tagging_parse_real_nextest_output() {
    let suite_started = r#"{"type":"suite","event":"started","test_count":6,"nextest":{"crate":"sample-crate","test_binary":"sample_crate","kind":"lib"}}"#;

    let parsed: NextestMessage = from_str(suite_started).unwrap();

    match parsed {
        NextestMessage::Suite(SuiteEvent::Started {
            test_count,
            nextest,
        }) => {
            assert_eq!(test_count, 6);
            assert_eq!(nextest.krate, "sample-crate");
            assert_eq!(nextest.test_binary, "sample_crate");
            assert_eq!(nextest.kind, "lib");
        }
        other => panic!("Expected Suite(Started), got {other:?}"),
    }
}

#[test]
fn nested_internal_tagging_parse_test_started() {
    let test_started = r#"{"type":"test","event":"started","name":"sample-crate::sample_crate$tests::test_passing"}"#;

    let parsed: NextestMessage = from_str(test_started).unwrap();

    match parsed {
        NextestMessage::Test(TestEvent::Started { name }) => {
            assert_eq!(name, "sample-crate::sample_crate$tests::test_passing");
        }
        other => panic!("Expected Test(Started), got {other:?}"),
    }
}

#[test]
fn nested_internal_tagging_parse_test_ok() {
    let test_ok = r#"{"type":"test","event":"ok","name":"sample-crate::sample_crate$tests::test_passing","exec_time":0.006818375}"#;

    let parsed: NextestMessage = from_str(test_ok).unwrap();

    match parsed {
        NextestMessage::Test(TestEvent::Ok { name, exec_time }) => {
            assert_eq!(name, "sample-crate::sample_crate$tests::test_passing");
            assert!((exec_time - 0.006818375).abs() < 0.0001);
        }
        other => panic!("Expected Test(Ok), got {other:?}"),
    }
}

#[test]
fn nested_internal_tagging_parse_test_failed() {
    let test_failed = r#"{"type":"test","event":"failed","name":"sample-crate::sample_crate$tests::test_panic_in_nested_call","exec_time":0.022801875,"stdout":"\nthread 'tests::test_panic_in_nested_call' panicked at src/lib.rs:10:5:\nsomething went wrong\n"}"#;

    let parsed: NextestMessage = from_str(test_failed).unwrap();

    match parsed {
        NextestMessage::Test(TestEvent::Failed {
            name,
            exec_time,
            stdout,
        }) => {
            assert_eq!(
                name,
                "sample-crate::sample_crate$tests::test_panic_in_nested_call"
            );
            assert!((exec_time - 0.022801875).abs() < 0.0001);
            assert!(stdout.contains("panicked at src/lib.rs:10:5"));
        }
        other => panic!("Expected Test(Failed), got {other:?}"),
    }
}

#[test]
fn nested_internal_tagging_parse_suite_failed() {
    let suite_failed = r#"{"type":"suite","event":"failed","passed":1,"failed":5,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.120977625,"nextest":{"crate":"sample-crate","test_binary":"sample_crate","kind":"lib"}}"#;

    let parsed: NextestMessage = from_str(suite_failed).unwrap();

    match parsed {
        NextestMessage::Suite(SuiteEvent::Failed {
            passed,
            failed,
            ignored,
            exec_time,
            nextest,
        }) => {
            assert_eq!(passed, 1);
            assert_eq!(failed, 5);
            assert_eq!(ignored, 0);
            assert!((exec_time - 0.120977625).abs() < 0.0001);
            assert_eq!(nextest.krate, "sample-crate");
        }
        other => panic!("Expected Suite(Failed), got {other:?}"),
    }
}
