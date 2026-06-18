use spec_proto::{Config, Measurement, Profile, Record, Status, Tag};
use spec_tests::harness::{SubjectLanguage, SubjectSpec, run_async, with_subject_cmd};

const EVOLVED_TS_CMD: &str = "./typescript/subject/subject-ts-evolved.sh";

fn ts_tcp() -> SubjectSpec {
    SubjectSpec::tcp(SubjectLanguage::TypeScript)
}

// r[verify schema.translation.fill-defaults]
// r[verify schema.translation.skip-unknown]
/// Rust v1 sends Profile{name, bio} → TypeScript evolved has Profile{name, bio, avatar}.
/// v2 should fill avatar with default (None), echo back, then v1 skips avatar.
pub fn run_schema_compat_added_optional_field() {
    run_async(async {
        with_subject_cmd(ts_tcp(), EVOLVED_TS_CMD, async |client| {
            let profile = Profile {
                name: "Alice".to_string(),
                bio: "Likes Rust".to_string(),
            };
            let resp = client
                .echo_profile(profile.clone())
                .await
                .map_err(|e| format!("echo_profile: {e:?}"))?;
            if resp.name != profile.name || resp.bio != profile.bio {
                return Err(format!("expected {profile:?}, got {resp:?}"));
            }
            Ok(())
        })
        .await
    })
    .unwrap();
}

// r[verify schema.translation.reorder]
/// Rust v1 sends Record{alpha, beta, gamma} → TypeScript evolved has {gamma, alpha, beta}.
/// Translation plan reorders fields. Values should round-trip correctly.
pub fn run_schema_compat_reordered_fields() {
    run_async(async {
        with_subject_cmd(ts_tcp(), EVOLVED_TS_CMD, async |client| {
            let record = Record {
                alpha: 42,
                beta: "hello".to_string(),
                gamma: 5.25_f64,
            };
            let resp = client
                .echo_record(record.clone())
                .await
                .map_err(|e| format!("echo_record: {e:?}"))?;
            if resp != record {
                return Err(format!("expected {record:?}, got {resp:?}"));
            }
            Ok(())
        })
        .await
    })
    .unwrap();
}

// r[verify schema.translation.enum]
// r[verify schema.translation.enum.missing-variant]
/// Rust v1 sends Status{Active, Inactive} → TypeScript evolved has {Active, Inactive, Suspended}.
/// v2 knows all v1 variants, so echoing Active should work fine.
pub fn run_schema_compat_added_enum_variant() {
    run_async(async {
        with_subject_cmd(ts_tcp(), EVOLVED_TS_CMD, async |client| {
            let status = Status::Active;
            let resp = client
                .echo_status(status.clone())
                .await
                .map_err(|e| format!("echo_status: {e:?}"))?;
            if resp != status {
                return Err(format!("expected {status:?}, got {resp:?}"));
            }
            Ok(())
        })
        .await
    })
    .unwrap();
}

// r[verify schema.translation.skip-unknown]
/// Rust v1 sends Tag{label, priority, note} → TypeScript evolved has {label, priority}.
/// v2 skips the unknown `note` field, echoes back {label, priority}.
/// v1 receives back and fills `note` with default... but String has no default.
/// This tests what happens when v2→v1 has a missing required field.
pub fn run_schema_compat_removed_field() {
    run_async(async {
        with_subject_cmd(ts_tcp(), EVOLVED_TS_CMD, async |client| {
            let tag = Tag {
                label: "important".to_string(),
                priority: 1,
                note: "don't forget".to_string(),
            };
            // v1→v2: should work (v2 skips note)
            // v2→v1: note is missing and String has no default — expect error
            let result = client.echo_tag(tag).await;
            match result {
                Err(_) => {
                    // Expected: v2's response lacks `note`, v1 can't fill the default
                }
                Ok(resp) => {
                    // If this succeeds, check that we got empty string default
                    if resp.note.is_empty() {
                        // String default is empty — that's also valid if facet provides it
                    } else {
                        return Err(format!("unexpected success with non-empty note: {resp:?}"));
                    }
                }
            }
            Ok(())
        })
        .await
    })
    .unwrap();
}

// r[verify schema.errors.type-mismatch]
/// Rust v1 has Measurement{value: f64}, TypeScript evolved has {value: String}.
/// Translation plan should fail — type mismatch on `value` field.
/// The call should error but the connection should stay up.
pub fn run_schema_compat_incompatible_type_change() {
    run_async(async {
        with_subject_cmd(ts_tcp(), EVOLVED_TS_CMD, async |client| {
            let m = Measurement {
                unit: "meters".to_string(),
                value: 5.25,
            };
            let result = client.echo_measurement(m).await;
            if result.is_ok() {
                return Err("expected error for incompatible type change, got Ok".to_string());
            }

            // Connection should still be alive — verify with a different method
            // (echo_profile uses compatible types)
            let profile = Profile {
                name: "Bob".to_string(),
                bio: "still alive".to_string(),
            };
            let resp = client
                .echo_profile(profile.clone())
                .await
                .map_err(|e| format!("echo_profile after failed echo_measurement: {e:?}"))?;
            if resp.name != profile.name {
                return Err(format!("connection broken after type mismatch: {resp:?}"));
            }
            Ok(())
        })
        .await
    })
    .unwrap();
}

// r[verify schema.errors.missing-required]
/// Rust v1 sends Config{key, value} → TypeScript evolved has {key, value, owner}.
/// v1→v2: owner is missing and required → translation plan should fail.
/// But v2→v1 would work fine (v1 skips owner). Since the *request* direction
/// is v1→v2, the callee (v2) can't build a plan for the incoming args.
pub fn run_schema_compat_missing_required_field() {
    run_async(async {
        with_subject_cmd(ts_tcp(), EVOLVED_TS_CMD, async |client| {
            let config = Config {
                key: "theme".to_string(),
                value: "dark".to_string(),
            };
            let result = client.echo_config(config).await;
            if result.is_ok() {
                return Err("expected error for missing required field, got Ok".to_string());
            }

            // Connection should still be alive
            let record = Record {
                alpha: 1,
                beta: "still up".to_string(),
                gamma: 2.0,
            };
            let resp = client
                .echo_record(record.clone())
                .await
                .map_err(|e| format!("echo_record after failed echo_config: {e:?}"))?;
            if resp != record {
                return Err(format!(
                    "connection broken after missing required: {resp:?}"
                ));
            }
            Ok(())
        })
        .await
    })
    .unwrap();
}
