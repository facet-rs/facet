//! Repro for: Option<SomeStruct> as a method return type round-trips
//! as an empty payload, causing the client to fail with
//! `InvalidPayload("unexpected EOF at byte 0")`.
//!
//! Discovered while building stax-server. Vec<SomeStruct> works fine
//! at the same call site, so the bug is specific to Option of a
//! struct (Option<u64>, Option<String> haven't been exercised here
//! but may or may not behave the same).

use std::time::Duration;

use facet::Facet;
use vox::memory_link_pair;

/// Nested types mirroring stax-server's `RunSummary` — that's where
/// we hit the bug originally. A unit-or-struct variants enum carried
/// inside an Option, an Option<u64>, and an Option<u32>.
#[derive(Clone, Debug, Facet, PartialEq)]
#[repr(u8)]
pub enum Reason {
    Plain,
    WithMessage { message: String },
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Inner {
    pub a: u64,
    pub b: String,
    pub maybe_reason: Option<Reason>,
    pub maybe_pid: Option<u32>,
    pub maybe_stamp: Option<u64>,
}

/// Convenience: build the same shape stax-server returned for an
/// active run — every Option populated.
fn populated() -> Inner {
    Inner {
        a: 42,
        b: "hello".to_owned(),
        maybe_reason: Some(Reason::WithMessage {
            message: "boom".to_owned(),
        }),
        maybe_pid: Some(1234),
        maybe_stamp: Some(123_456_789_000),
    }
}

/// And the active=None equivalent — every nested Option None too,
/// approximating the empty-server case.
fn empty() -> Inner {
    Inner {
        a: 0,
        b: String::new(),
        maybe_reason: None,
        maybe_pid: None,
        maybe_stamp: None,
    }
}

#[vox::service]
pub trait OptionRepro {
    /// Returns `Some(_)` when `present` is true, `None` otherwise.
    /// The `None` case is what trips the bug — the response payload
    /// is observed empty on the client side and deserialization
    /// fails before our code sees an Option.
    async fn option_struct(&self, present: bool) -> Option<Inner>;

    /// Control: same shape but Vec instead of Option. This works.
    async fn vec_struct(&self, n: u32) -> Vec<Inner>;
}

#[derive(Clone)]
struct ReproService;

impl OptionRepro for ReproService {
    async fn option_struct(&self, present: bool) -> Option<Inner> {
        present.then(populated)
    }

    async fn vec_struct(&self, n: u32) -> Vec<Inner> {
        (0..n)
            .map(|i| Inner {
                a: i as u64,
                b: format!("entry-{i}"),
                maybe_reason: None,
                maybe_pid: None,
                maybe_stamp: None,
            })
            .collect()
    }
}

async fn pair() -> (OptionReproClient, vox::NoopClient) {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(OptionReproDispatcher::new(ReproService))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish")
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<OptionReproClient>()
        .await
        .expect("client establish");

    let server_guard = server.await.expect("server task");
    (client, server_guard)
}

#[tokio::test]
async fn vec_of_struct_roundtrips_fine_empty() {
    let (client, _server) = pair().await;
    let got = client.vec_struct(0).await.expect("vec_struct call");
    assert!(got.is_empty());
}

#[tokio::test]
async fn vec_of_struct_roundtrips_fine_nonempty() {
    let (client, _server) = pair().await;
    let got = client.vec_struct(3).await.expect("vec_struct call");
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].a, 0);
    assert_eq!(got[2].b, "entry-2");
}

/// This is the bug. With `Option<Inner>` returned as `Some(_)`,
/// expect Some to come back. (Often passes — the failure mode I hit
/// was the `None` case, see below.)
#[tokio::test]
async fn option_of_struct_some_roundtrips() {
    let (client, _server) = pair().await;
    let got = tokio::time::timeout(Duration::from_secs(5), client.option_struct(true))
        .await
        .expect("returned within 5s")
        .expect("option_struct call");
    assert_eq!(got, Some(populated()));
}

/// Sanity: stop tripping over the obvious — a single Inner-shaped
/// struct returned bare (no Option / Vec wrapper) round-trips fine.
#[vox::service]
trait Bare {
    async fn one(&self, present: bool) -> Inner;
}

#[derive(Clone)]
struct BareService;

impl Bare for BareService {
    async fn one(&self, present: bool) -> Inner {
        if present { populated() } else { empty() }
    }
}

async fn bare_pair() -> (BareClient, vox::NoopClient) {
    let (client_link, server_link) = memory_link_pair(16);
    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(BareDispatcher::new(BareService))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish")
    });
    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<BareClient>()
        .await
        .expect("client establish");
    let server_guard = server.await.expect("server task");
    (client, server_guard)
}

#[tokio::test]
async fn bare_struct_all_nested_options_none() {
    let (client, _server) = bare_pair().await;
    let got = tokio::time::timeout(Duration::from_secs(5), client.one(false))
        .await
        .expect("returned within 5s")
        .expect("call");
    assert_eq!(got, empty());
}

#[tokio::test]
async fn bare_struct_all_nested_options_some() {
    let (client, _server) = bare_pair().await;
    let got = tokio::time::timeout(Duration::from_secs(5), client.one(true))
        .await
        .expect("returned within 5s")
        .expect("call");
    assert_eq!(got, populated());
}

/// Narrower: just `Option<Reason>` directly, no enclosing struct.
/// Returning `Some(WithMessage{...})` and `None` should both
/// round-trip cleanly.
#[vox::service]
trait OptionEnum {
    async fn opt_reason(&self, present: bool) -> Option<Reason>;
}

#[derive(Clone)]
struct OptionEnumService;

impl OptionEnum for OptionEnumService {
    async fn opt_reason(&self, present: bool) -> Option<Reason> {
        present.then(|| Reason::WithMessage {
            message: "boom".to_owned(),
        })
    }
}

#[tokio::test]
async fn option_of_enum_with_data_variant_roundtrips_some() {
    let (client_link, server_link) = memory_link_pair(16);
    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(OptionEnumDispatcher::new(OptionEnumService))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish")
    });
    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<OptionEnumClient>()
        .await
        .expect("client establish");
    let _server = server.await.expect("server task");
    let got = tokio::time::timeout(Duration::from_secs(5), client.opt_reason(true))
        .await
        .expect("returned within 5s")
        .expect("call");
    assert_eq!(
        got,
        Some(Reason::WithMessage {
            message: "boom".to_owned()
        })
    );
}

/// The failing case. Returning `None` from the server side never
/// resolves on the client side — the call hangs forever rather than
/// either deserialising as `Ok(None)` or producing a typed error.
/// (When the connection is later torn down, callers see
/// `InvalidPayload("unexpected EOF at byte 0")`.)
///
/// Wrapped in a 5-second timeout so the test fails cleanly instead
/// of pinning the whole test suite.
#[tokio::test]
async fn option_of_struct_none_roundtrips() {
    let (client, _server) = pair().await;
    let got = tokio::time::timeout(Duration::from_secs(5), client.option_struct(false))
        .await
        .expect("option_struct(false) returned within 5s")
        .expect("option_struct call");
    assert_eq!(got, None);
}
