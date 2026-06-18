//! Evolved versions of testbed types for schema compatibility testing.
//!
//! These types have the same names as the v1 types in the parent module but
//! with structural differences. Schema exchange + translation plans should
//! bridge compatible differences; incompatible differences should fail at the
//! method call level without tearing down the connection.
//!
//! The `Testbed` trait here has the same service name and method names as the
//! v1 trait, so method IDs match. Only the types differ.

use facet::Facet;
use vox::service;

/// Evolved testbed service — same method names, different types.
#[service]
pub trait Testbed {
    /// Echo a profile back. Tests added optional field.
    async fn echo_profile(&self, profile: Profile) -> Profile;

    /// Echo a record back. Tests field reordering.
    async fn echo_record(&self, record: Record) -> Record;

    /// Echo a status back. Tests added enum variant.
    async fn echo_status(&self, status: Status) -> Status;

    /// Echo a tag back. Tests removed field.
    async fn echo_tag(&self, tag: Tag) -> Tag;

    /// Echo a measurement back. Tests incompatible type change.
    async fn echo_measurement(&self, m: Measurement) -> Measurement;

    /// Echo a config back. Tests missing required field.
    async fn echo_config(&self, c: Config) -> Config;
}

/// Added optional field: v1 has {name, bio}, v2 adds {avatar}.
/// Compatible in both directions:
/// - v1→v2: avatar filled with default (None)
/// - v2→v1: avatar skipped
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Profile {
    pub name: String,
    pub bio: String,
    pub avatar: Option<String>,
}

/// Reordered fields: v1 has {alpha, beta, gamma}, v2 has {gamma, alpha, beta}.
/// Compatible in both directions — translation plan reorders.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Record {
    pub gamma: f64,
    pub alpha: i32,
    pub beta: String,
}

/// Added enum variant: v1 has {Active, Inactive}, v2 adds {Suspended}.
/// - v1→v2: always works (v2 knows all v1 variants)
/// - v2→v1: works unless Suspended is actually sent (runtime error per
///   r[schema.translation.enum.unknown-variant])
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Inactive = 1,
    Suspended = 2,
}

/// Removed field: v1 has {label, priority, note}, v2 drops {note}.
/// - v1→v2: note is skipped (r[schema.translation.skip-unknown])
/// - v2→v1: note filled with default (r[schema.translation.fill-defaults])
///   NOTE: String has no Facet default, so v2→v1 will fail at plan time
///   unless we make note optional in v1. This tests the missing-required error.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Tag {
    pub label: String,
    pub priority: u32,
}

/// Incompatible type change: v1 has {value: f64}, v2 has {value: String}.
/// Should fail at translation plan construction (r[schema.errors.type-mismatch]).
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Measurement {
    pub unit: String,
    pub value: String,
}

/// Added required field: v1 has {key, value}, v2 adds {owner: String}.
/// - v2→v1: owner is skipped (works fine)
/// - v1→v2: owner is missing and required → plan fails (r[schema.errors.missing-required])
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Config {
    pub key: String,
    pub value: String,
    pub owner: String,
}
