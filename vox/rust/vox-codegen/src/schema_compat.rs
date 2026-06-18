//! Service schema snapshot and compatibility tooling.
//!
//! Vox runtime compatibility is executed by phon decode plans. This module is the
//! tooling layer over the same semantic primitive: snapshot a service's phon
//! schema closures, compare two snapshots by attempting phon compatibility plans
//! in both directions, and enforce a project policy over the reported facts.

use std::collections::{BTreeMap, BTreeSet};

use facet::Facet;
use phon_engine::Registry;
use phon_engine::plan::{CompatDirection, compatibility_direction};
use phon_schema::{Schema, SchemaId};
use vox_types::{BindingDirection, ServiceDescriptor};

#[derive(Debug)]
pub enum SchemaCompatError {
    Snapshot {
        service: &'static str,
        method: &'static str,
        direction: BindingDirection,
        message: String,
    },
    Hex(String),
    SchemaClosure(String),
}

impl std::fmt::Display for SchemaCompatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot {
                service,
                method,
                direction,
                message,
            } => write!(
                f,
                "failed to snapshot {service}.{method} {direction:?}: {message}"
            ),
            Self::Hex(message) => write!(f, "{message}"),
            Self::SchemaClosure(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SchemaCompatError {}

#[derive(Facet, Clone, Debug)]
pub struct SchemaCompatSnapshot {
    pub services: Vec<ServiceSchemaSnapshot>,
}

#[derive(Facet, Clone, Debug)]
pub struct ServiceSchemaSnapshot {
    pub service_name: String,
    pub methods: Vec<MethodSchemaSnapshot>,
}

#[derive(Facet, Clone, Debug)]
pub struct MethodSchemaSnapshot {
    pub method_name: String,
    pub method_id: String,
    pub args: SchemaRootSnapshot,
    pub response: SchemaRootSnapshot,
}

#[derive(Facet, Clone, Debug)]
pub struct SchemaRootSnapshot {
    pub root: String,
    pub closure: String,
}

#[derive(Facet, Clone, Debug)]
pub struct SchemaCompatReport {
    pub comparisons: Vec<SchemaCompatComparison>,
}

#[derive(Facet, Clone, Debug)]
pub struct SchemaCompatComparison {
    pub service_name: String,
    pub method_name: String,
    pub direction: SchemaCompatComparisonDirection,
    pub status: SchemaCompatStatus,
    pub note: Option<String>,
}

#[derive(Facet, Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum SchemaCompatComparisonDirection {
    Args,
    Response,
    Method,
}

#[derive(Facet, Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum SchemaCompatStatus {
    Backward,
    Forward,
    Bidirectional,
    Incompatible,
}

impl From<CompatDirection> for SchemaCompatStatus {
    fn from(value: CompatDirection) -> Self {
        match value {
            CompatDirection::Backward => Self::Backward,
            CompatDirection::Forward => Self::Forward,
            CompatDirection::Bidirectional => Self::Bidirectional,
            CompatDirection::Incompatible => Self::Incompatible,
        }
    }
}

#[derive(Facet, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SchemaCompatAcknowledgement {
    pub service_name: String,
    pub method_name: String,
    pub direction: SchemaCompatComparisonDirection,
}

#[derive(Facet, Clone, Debug)]
pub struct SchemaCompatPolicy {
    pub acknowledged_breaking: Vec<SchemaCompatAcknowledgement>,
}

#[derive(Facet, Clone, Debug)]
pub struct SchemaCompatPolicyOutcome {
    pub unacknowledged_breaking: Vec<SchemaCompatComparison>,
    pub stale_acknowledgements: Vec<SchemaCompatAcknowledgement>,
}

impl SchemaCompatPolicyOutcome {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.unacknowledged_breaking.is_empty() && self.stale_acknowledgements.is_empty()
    }
}

// r[impl schema.compat.snapshot]
pub fn snapshot_services(
    services: &[&'static ServiceDescriptor],
) -> Result<SchemaCompatSnapshot, SchemaCompatError> {
    let mut service_snapshots = Vec::with_capacity(services.len());
    for service in services {
        let mut methods = Vec::with_capacity(service.methods.len());
        for method in service.methods {
            let args = snapshot_root(
                service.service_name,
                method.method_name,
                BindingDirection::Args,
                method.args_shape,
            )?;
            let response = snapshot_root(
                service.service_name,
                method.method_name,
                BindingDirection::Response,
                method.response_wire_shape,
            )?;
            methods.push(MethodSchemaSnapshot {
                method_name: method.method_name.to_string(),
                method_id: hex_u64(method.id.0),
                args,
                response,
            });
        }
        service_snapshots.push(ServiceSchemaSnapshot {
            service_name: service.service_name.to_string(),
            methods,
        });
    }
    Ok(SchemaCompatSnapshot {
        services: service_snapshots,
    })
}

// r[impl schema.compat.check]
// r[impl rpc.schema-evolution]
pub fn compare_snapshots(
    older: &SchemaCompatSnapshot,
    newer: &SchemaCompatSnapshot,
) -> Result<SchemaCompatReport, SchemaCompatError> {
    let registry = registry_for_snapshots(older, newer)?;
    let mut comparisons = Vec::new();

    let old_services = service_map(older);
    let new_services = service_map(newer);
    let service_names = old_services
        .keys()
        .chain(new_services.keys())
        .copied()
        .collect::<BTreeSet<_>>();

    for service_name in service_names {
        match (
            old_services.get(service_name).copied(),
            new_services.get(service_name).copied(),
        ) {
            (Some(old_service), Some(new_service)) => {
                compare_service(old_service, new_service, &registry, &mut comparisons)?;
            }
            (Some(_), None) => comparisons.push(SchemaCompatComparison {
                service_name: service_name.to_string(),
                method_name: "*".to_string(),
                direction: SchemaCompatComparisonDirection::Method,
                status: SchemaCompatStatus::Incompatible,
                note: Some("service removed".to_string()),
            }),
            (None, Some(_)) => comparisons.push(SchemaCompatComparison {
                service_name: service_name.to_string(),
                method_name: "*".to_string(),
                direction: SchemaCompatComparisonDirection::Method,
                status: SchemaCompatStatus::Bidirectional,
                note: Some("service added".to_string()),
            }),
            (None, None) => unreachable!("service name came from one of the maps"),
        }
    }

    Ok(SchemaCompatReport { comparisons })
}

pub fn method_intersection_snapshot(
    snapshot: &SchemaCompatSnapshot,
    reference: &SchemaCompatSnapshot,
) -> SchemaCompatSnapshot {
    let reference_services = service_map(reference);
    let services = snapshot
        .services
        .iter()
        .filter_map(|service| {
            let reference_service = reference_services.get(service.service_name.as_str())?;
            let reference_methods = reference_service
                .methods
                .iter()
                .map(|method| method.method_name.as_str())
                .collect::<BTreeSet<_>>();
            let methods = service
                .methods
                .iter()
                .filter(|method| reference_methods.contains(method.method_name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            Some(ServiceSchemaSnapshot {
                service_name: service.service_name.clone(),
                methods,
            })
        })
        .collect();
    SchemaCompatSnapshot { services }
}

// r[impl schema.compat.policy]
#[must_use]
pub fn enforce_policy(
    report: &SchemaCompatReport,
    policy: &SchemaCompatPolicy,
) -> SchemaCompatPolicyOutcome {
    let acknowledged = policy
        .acknowledged_breaking
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let actual_breaking = report
        .comparisons
        .iter()
        .filter(|comparison| comparison.status == SchemaCompatStatus::Incompatible)
        .map(|comparison| SchemaCompatAcknowledgement {
            service_name: comparison.service_name.clone(),
            method_name: comparison.method_name.clone(),
            direction: comparison.direction,
        })
        .collect::<BTreeSet<_>>();

    let unacknowledged_breaking = report
        .comparisons
        .iter()
        .filter(|comparison| comparison.status == SchemaCompatStatus::Incompatible)
        .filter(|comparison| {
            !acknowledged.contains(&SchemaCompatAcknowledgement {
                service_name: comparison.service_name.clone(),
                method_name: comparison.method_name.clone(),
                direction: comparison.direction,
            })
        })
        .cloned()
        .collect();

    let stale_acknowledgements = acknowledged.difference(&actual_breaking).cloned().collect();

    SchemaCompatPolicyOutcome {
        unacknowledged_breaking,
        stale_acknowledgements,
    }
}

fn compare_service(
    old_service: &ServiceSchemaSnapshot,
    new_service: &ServiceSchemaSnapshot,
    registry: &Registry,
    comparisons: &mut Vec<SchemaCompatComparison>,
) -> Result<(), SchemaCompatError> {
    let old_methods = method_map(old_service);
    let new_methods = method_map(new_service);
    let method_names = old_methods
        .keys()
        .chain(new_methods.keys())
        .copied()
        .collect::<BTreeSet<_>>();

    for method_name in method_names {
        match (
            old_methods.get(method_name).copied(),
            new_methods.get(method_name).copied(),
        ) {
            (Some(old_method), Some(new_method)) => {
                compare_root(
                    &old_service.service_name,
                    method_name,
                    SchemaCompatComparisonDirection::Args,
                    &old_method.args,
                    &new_method.args,
                    registry,
                    comparisons,
                )?;
                compare_root(
                    &old_service.service_name,
                    method_name,
                    SchemaCompatComparisonDirection::Response,
                    &old_method.response,
                    &new_method.response,
                    registry,
                    comparisons,
                )?;
            }
            (Some(_), None) => comparisons.push(SchemaCompatComparison {
                service_name: old_service.service_name.clone(),
                method_name: method_name.to_string(),
                direction: SchemaCompatComparisonDirection::Method,
                status: SchemaCompatStatus::Incompatible,
                note: Some("method removed".to_string()),
            }),
            (None, Some(_)) => comparisons.push(SchemaCompatComparison {
                service_name: new_service.service_name.clone(),
                method_name: method_name.to_string(),
                direction: SchemaCompatComparisonDirection::Method,
                status: SchemaCompatStatus::Bidirectional,
                note: Some("method added".to_string()),
            }),
            (None, None) => unreachable!("method name came from one of the maps"),
        }
    }

    Ok(())
}

fn compare_root(
    service_name: &str,
    method_name: &str,
    direction: SchemaCompatComparisonDirection,
    older: &SchemaRootSnapshot,
    newer: &SchemaRootSnapshot,
    registry: &Registry,
    comparisons: &mut Vec<SchemaCompatComparison>,
) -> Result<(), SchemaCompatError> {
    let older_root = parse_schema_id(&older.root)?;
    let newer_root = parse_schema_id(&newer.root)?;
    comparisons.push(SchemaCompatComparison {
        service_name: service_name.to_string(),
        method_name: method_name.to_string(),
        direction,
        status: compatibility_direction(older_root, newer_root, registry).into(),
        note: None,
    });
    Ok(())
}

fn snapshot_root(
    service: &'static str,
    method: &'static str,
    direction: BindingDirection,
    shape: &'static facet::Shape,
) -> Result<SchemaRootSnapshot, SchemaCompatError> {
    let closure =
        vox_phon::schema_bytes_for_shape(shape).map_err(|error| SchemaCompatError::Snapshot {
            service,
            method,
            direction,
            message: error.to_string(),
        })?;
    let bundle =
        vox_phon::parse_schema_bytes(&closure).map_err(|error| SchemaCompatError::Snapshot {
            service,
            method,
            direction,
            message: error.to_string(),
        })?;
    Ok(SchemaRootSnapshot {
        root: hex_u64(bundle.root.0),
        closure: hex_bytes(&closure),
    })
}

fn registry_for_snapshots(
    older: &SchemaCompatSnapshot,
    newer: &SchemaCompatSnapshot,
) -> Result<Registry, SchemaCompatError> {
    let mut schemas = BTreeMap::<u64, Schema>::new();
    for root in roots(older).chain(roots(newer)) {
        let closure = parse_hex(&root.closure)?;
        let bundle = vox_phon::parse_schema_bytes(&closure)
            .map_err(|error| SchemaCompatError::SchemaClosure(error.to_string()))?;
        let expected_root = parse_schema_id(&root.root)?;
        if bundle.root != expected_root {
            return Err(SchemaCompatError::SchemaClosure(format!(
                "schema closure root {} did not match snapshot root {}",
                hex_u64(bundle.root.0),
                root.root
            )));
        }
        for schema in bundle.schemas {
            schemas.entry(schema.id.0).or_insert(schema);
        }
    }
    Ok(Registry::new(schemas.into_values()))
}

fn roots(snapshot: &SchemaCompatSnapshot) -> impl Iterator<Item = &SchemaRootSnapshot> {
    snapshot.services.iter().flat_map(|service| {
        service
            .methods
            .iter()
            .flat_map(|method| [&method.args, &method.response])
    })
}

fn service_map(snapshot: &SchemaCompatSnapshot) -> BTreeMap<&str, &ServiceSchemaSnapshot> {
    snapshot
        .services
        .iter()
        .map(|service| (service.service_name.as_str(), service))
        .collect()
}

fn method_map(service: &ServiceSchemaSnapshot) -> BTreeMap<&str, &MethodSchemaSnapshot> {
    service
        .methods
        .iter()
        .map(|method| (method.method_name.as_str(), method))
        .collect()
}

fn parse_schema_id(hex: &str) -> Result<SchemaId, SchemaCompatError> {
    let trimmed = hex.strip_prefix("0x").unwrap_or(hex);
    u64::from_str_radix(trimmed, 16)
        .map(SchemaId)
        .map_err(|error| SchemaCompatError::Hex(format!("invalid schema id {hex:?}: {error}")))
}

fn parse_hex(hex: &str) -> Result<Vec<u8>, SchemaCompatError> {
    let trimmed = hex.strip_prefix("0x").unwrap_or(hex);
    if !trimmed.len().is_multiple_of(2) {
        return Err(SchemaCompatError::Hex(format!(
            "hex string has odd length: {hex:?}"
        )));
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    let bytes = trimmed.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8, SchemaCompatError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(SchemaCompatError::Hex(format!(
            "invalid hex digit {:?}",
            char::from(byte)
        ))),
    }
}

fn hex_u64(value: u64) -> String {
    format!("{value:016x}")
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn testbed_policy() -> SchemaCompatPolicy {
        SchemaCompatPolicy {
            acknowledged_breaking: vec![
                SchemaCompatAcknowledgement {
                    service_name: "Testbed".to_string(),
                    method_name: "echo_measurement".to_string(),
                    direction: SchemaCompatComparisonDirection::Args,
                },
                SchemaCompatAcknowledgement {
                    service_name: "Testbed".to_string(),
                    method_name: "echo_measurement".to_string(),
                    direction: SchemaCompatComparisonDirection::Response,
                },
            ],
        }
    }

    fn evolved_testbed_snapshots() -> (SchemaCompatSnapshot, SchemaCompatSnapshot) {
        let old_services = spec_proto::all_services();
        let old = snapshot_services(&old_services).expect("old snapshot");
        let evolved_services = [spec_proto::evolved::testbed_service_descriptor()];
        let new = snapshot_services(&evolved_services).expect("new snapshot");
        (method_intersection_snapshot(&old, &new), new)
    }

    fn snapshot_without_method(
        snapshot: &SchemaCompatSnapshot,
        service_name: &str,
        method_name: &str,
    ) -> SchemaCompatSnapshot {
        let mut snapshot = snapshot.clone();
        let service = snapshot
            .services
            .iter_mut()
            .find(|service| service.service_name == service_name)
            .expect("service in snapshot");
        let before = service.methods.len();
        service
            .methods
            .retain(|method| method.method_name != method_name);
        assert_eq!(
            service.methods.len(),
            before - 1,
            "method {service_name}.{method_name} should exist in snapshot"
        );
        snapshot
    }

    // r[verify schema.compat.snapshot]
    #[test]
    fn snapshots_include_method_args_and_response_roots() {
        let services = spec_proto::all_services();
        let snapshot = snapshot_services(&services).expect("snapshot services");
        let testbed = snapshot
            .services
            .iter()
            .find(|service| service.service_name == "Testbed")
            .expect("testbed service");
        let echo_profile = testbed
            .methods
            .iter()
            .find(|method| method.method_name == "echo_profile")
            .expect("echo_profile method");

        assert!(!echo_profile.args.root.is_empty());
        assert!(!echo_profile.args.closure.is_empty());
        assert!(!echo_profile.response.root.is_empty());
        assert!(!echo_profile.response.closure.is_empty());
    }

    // r[verify schema.compat.check]
    // r[verify rpc.schema-evolution]
    #[test]
    fn compares_evolved_testbed_schema_directions() {
        let (old, new) = evolved_testbed_snapshots();
        let report = compare_snapshots(&old, &new).expect("compat report");

        let echo_profile_args = report
            .comparisons
            .iter()
            .find(|comparison| {
                comparison.method_name == "echo_profile"
                    && comparison.direction == SchemaCompatComparisonDirection::Args
            })
            .expect("echo_profile args comparison");
        assert_eq!(echo_profile_args.status, SchemaCompatStatus::Bidirectional);

        let echo_measurement_args = report
            .comparisons
            .iter()
            .find(|comparison| {
                comparison.method_name == "echo_measurement"
                    && comparison.direction == SchemaCompatComparisonDirection::Args
            })
            .expect("echo_measurement args comparison");
        assert_eq!(
            echo_measurement_args.status,
            SchemaCompatStatus::Incompatible
        );
    }

    // r[verify rpc.schema-evolution]
    #[test]
    fn compare_reports_added_methods_as_compatible() {
        let services = spec_proto::all_services();
        let new = snapshot_services(&services).expect("new snapshot");
        let old = snapshot_without_method(&new, "Testbed", "echo_profile");
        let report = compare_snapshots(&old, &new).expect("compat report");

        let added_method = report
            .comparisons
            .iter()
            .find(|comparison| {
                comparison.service_name == "Testbed"
                    && comparison.method_name == "echo_profile"
                    && comparison.direction == SchemaCompatComparisonDirection::Method
            })
            .expect("added method comparison");
        assert_eq!(added_method.status, SchemaCompatStatus::Bidirectional);
        assert_eq!(added_method.note.as_deref(), Some("method added"));
    }

    // r[verify rpc.schema-evolution]
    #[test]
    fn compare_reports_removed_methods_as_breaking() {
        let services = spec_proto::all_services();
        let old = snapshot_services(&services).expect("old snapshot");
        let new = snapshot_without_method(&old, "Testbed", "echo_profile");
        let report = compare_snapshots(&old, &new).expect("compat report");

        let removed_method = report
            .comparisons
            .iter()
            .find(|comparison| {
                comparison.service_name == "Testbed"
                    && comparison.method_name == "echo_profile"
                    && comparison.direction == SchemaCompatComparisonDirection::Method
            })
            .expect("removed method comparison");
        assert_eq!(removed_method.status, SchemaCompatStatus::Incompatible);
        assert_eq!(removed_method.note.as_deref(), Some("method removed"));
    }

    // r[verify schema.compat.policy]
    #[test]
    fn policy_requires_acknowledging_breaking_changes() {
        let (old, new) = evolved_testbed_snapshots();
        let report = compare_snapshots(&old, &new).expect("compat report");

        let rejected = enforce_policy(
            &report,
            &SchemaCompatPolicy {
                acknowledged_breaking: Vec::new(),
            },
        );
        assert!(!rejected.is_ok());
        assert!(!rejected.unacknowledged_breaking.is_empty());

        let accepted = enforce_policy(&report, &testbed_policy());
        assert!(accepted.is_ok(), "{accepted:?}");
    }
}
