//! Security conformance tests.
//!
//! Tests for spec rules in security.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// security.auth_failure_handshake
// =============================================================================
// Rules: [verify security.auth-failure.handshake]
//
// On auth failure: send CloseChannel, close transport, don't process other frames.

#[conformance(
    name = "security.auth_failure_handshake",
    rules = "security.auth-failure.handshake"
)]
pub async fn auth_failure_handshake(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.metadata_plaintext
// =============================================================================
// Rules: [verify security.metadata.plaintext]
//
// Hello params and OpenChannel metadata are NOT encrypted by Rapace.

#[conformance(
    name = "security.metadata_plaintext",
    rules = "security.metadata.plaintext"
)]
pub async fn metadata_plaintext(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.metadata_secrets
// =============================================================================
// Rules: [verify security.metadata.secrets]
//
// Implementations MUST NOT put sensitive data in metadata without transport encryption.

#[conformance(
    name = "security.metadata_secrets",
    rules = "security.metadata.secrets"
)]
pub async fn metadata_secrets(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.profile_a_multitenant
// =============================================================================
// Rules: [verify security.profile-a.multitenant]
//
// Multi-tenant deployments MUST still authenticate and authorize at the application layer.

#[conformance(
    name = "security.profile_a_multitenant",
    rules = "security.profile-a.multitenant"
)]
pub async fn profile_a_multitenant(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.profile_b_authenticate
// =============================================================================
// Rules: [verify security.profile-b.authenticate]
//
// Implementations MUST authenticate peers at the RPC layer.

#[conformance(
    name = "security.profile_b_authenticate",
    rules = "security.profile-b.authenticate"
)]
pub async fn profile_b_authenticate(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.profile_b_authorize
// =============================================================================
// Rules: [verify security.profile-b.authorize]
//
// Implementations MUST authorize each call based on the authenticated identity.

#[conformance(
    name = "security.profile_b_authorize",
    rules = "security.profile-b.authorize"
)]
pub async fn profile_b_authorize(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.profile_c_encryption
// =============================================================================
// Rules: [verify security.profile-c.encryption]
//
// Implementations MUST use confidentiality and integrity protection.

#[conformance(
    name = "security.profile_c_encryption",
    rules = "security.profile-c.encryption"
)]
pub async fn profile_c_encryption(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.profile_c_authenticate
// =============================================================================
// Rules: [verify security.profile-c.authenticate]
//
// Implementations MUST authenticate peers (mutual TLS, bearer tokens, etc.).

#[conformance(
    name = "security.profile_c_authenticate",
    rules = "security.profile-c.authenticate"
)]
pub async fn profile_c_authenticate(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// security.profile_c_reject
// =============================================================================
// Rules: [verify security.profile-c.reject]
//
// Implementations MUST reject connections with invalid or missing authentication.

#[conformance(
    name = "security.profile_c_reject",
    rules = "security.profile-c.reject"
)]
pub async fn profile_c_reject(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
