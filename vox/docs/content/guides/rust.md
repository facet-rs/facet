+++
title = "Rust Guide"
description = "Use vox + a transport crate to define services, run drivers, and call methods with channels."
weight = 21
+++

The best way to learn the Rust API is to run the examples in order, from simplest to most complex.

## 1) `borrowed_and_channels` (smallest complete RPC)

- Source: [rust-examples/examples/borrowed_and_channels.rs](https://github.com/bearcove/vox/blob/main/rust-examples/examples/borrowed_and_channels.rs)
- Run: `cargo run -p rust-examples --example borrowed_and_channels`
- Learn: borrowed args, borrowed returns, and `Rx<T>`/`Tx<T>` channel args.

> ```rust
> async fn is_short(&self, word: &str) -> bool;
> async fn classify(&self, word: String) -> &'vox str;
> async fn transform(&self, prefix: &str, input: Rx<String>, output: Tx<String>) -> u32;
> ```

## 2) `service_lanes` (multiple service lanes on one connection)

- Source: [rust-examples/examples/service_lanes.rs](https://github.com/bearcove/vox/blob/main/rust-examples/examples/service_lanes.rs)
- Run: `cargo run -p rust-examples --example service_lanes`
- Learn: structured `Decline`, connection identity resolution, lane grants,
  `open_lane`, and independent per-lane drivers.

> ```rust
> let authenticated_peer = authenticated_peer_label(request)?;
>
> match request.service() {
>     "CounterLab" => lane
>         .with_grant(vox::LaneGrant::from_metadata(
>             vox::metadata()
>                 .str("tenant", "lab")
>                 .str("grant-scope", "counter:read-write")
>                 .str("authenticated-peer", authenticated_peer)
>                 .build(),
>         ))
>         .handle_with(CounterLabDispatcher::new(counter)),
>     "StringLab" => lane
>         .with_grant(vox::LaneGrant::from_metadata(
>             vox::metadata()
>                 .str("tenant", "lab")
>                 .str("grant-scope", "string:read-write")
>                 .build(),
>         ))
>         .handle_with(StringLabDispatcher::new(strings)),
>     _ => return Err(vox::LaneRejection::with_message(
>         vox::LaneRejectReason::UnknownService,
>         "unknown service",
>     )),
> }
> ```

## 3) `reconnect` (attachment failure behavior)

- Source: [rust-examples/examples/reconnect.rs](https://github.com/bearcove/vox/blob/main/rust-examples/examples/reconnect.rs)
- Run: `cargo run -p rust-examples --example reconnect`
- Learn: behavior after a bare transport attachment is lost and a server is restarted.

> ```rust
> println!("[client] server killed");
> ...
> println!("[client] server restarted");
> ```

## 4) `memory_proxying` (lane-level proxying)

- Source: [rust-examples/examples/memory_proxying.rs](https://github.com/bearcove/vox/blob/main/rust-examples/examples/memory_proxying.rs)
- Run: `cargo run -p rust-examples --example memory_proxying`
- Learn: host bridges one service lane to another without service-specific forwarding code.

> ```rust
> vox::proxy_lanes(incoming_handle, upstream_lane).await;
> ```

- Learn: one host process launching two guest processes over local IPC, and serving different services from each guest.

> ```rust
> println!("[host] launching guest: Adder");
> println!("[host] launching guest: StringReverser");
> ```

## Practical API pattern

Most application code only needs `vox` + one transport crate.

```toml
[dependencies]
vox = "7.0.0"
vox-stream = "7.0.0"
tokio = { version = "1", features = ["rt", "net"] }
eyre = "0.6"
```

Define a service with `#[vox::service]`, implement it, and establish on each side:

```rust
let server_task = tokio::spawn(async move {
    vox::serve("127.0.0.1:9000", WordLabDispatcher::new(WordLabService)).await
});

let client: WordLabClient = vox::connect_lane("127.0.0.1:9000").await?;
```

## Connection policy

Handshake metadata carries early peer-authored claims. An identity resolver
verifies those claims against locally asserted transport evidence and either
returns the immutable connection identity or sends `Decline` during the
handshake:

```rust
let resolver = vox::identity_resolver_fn(|cx: vox::IdentityResolutionContext<'_>| {
    use vox::MetadataExt;

    match cx.claims.meta_str("-#authorization") {
        Some("Bearer local-dev") => Ok(vox::PeerIdentity::from_basis(
            vox::IdentityBasis::new(
                vox::PeerIdentityForm::ApplicationUser,
                vox::IdentityBasisProvenance::VerifiedClaimBacked,
                "local-dev-user",
            ),
        )),
        _ => Err(vox::Decline::new(
            vox::EstablishmentRejectReason::Unauthenticated,
        )),
    }
});

tokio::spawn(async move {
    vox::serve("127.0.0.1:9000", WordLabDispatcher::new(WordLabService))
        .identity_resolver(resolver)
        .await
});

let client = vox::connect("127.0.0.1:9000")
    .metadata(
        vox::metadata()
            .str("-#authorization", "Bearer local-dev")
            .build(),
    )
    .await?;
```

The peer that sends metadata does not verify its own metadata. In this example,
the connector authors the early claim and the acceptor resolves the connector's
identity from the peer claims it received. A connector-side resolver can also
verify acceptor metadata or transport evidence, but it is still verifying the
peer.

Late credentials in lane or request metadata do not rewrite the connection
identity. Verify them in lane/request policy and record the result in a lane
grant or request-local state.

Lane acceptors can attach local policy output to the lane. Request handlers
that opt into `RequestContext` can read the same grant for each call:

```rust
let acceptor = vox::lane_acceptor_fn(|request, lane| {
    let grant = vox::LaneGrant::from_metadata(
        vox::metadata()
            .str("tenant", request.service())
            .str("scope", "words:read")
            .build(),
    );
    lane.with_grant(grant)
        .handle_with(WordLabDispatcher::new(WordLabService));
    Ok(())
});

async fn describe(&self, cx: &vox::RequestContext<'_>) -> String {
    use vox::MetadataExt;

    let tenant = cx
        .authorization()
        .and_then(|auth| auth.lane_grant().metadata().meta_str("tenant").map(str::to_owned));
    tenant.unwrap_or_else(|| "anonymous".to_owned())
}
```

For borrowed returns, implementations receive a `Call` sink:

```rust
async fn classify<'vox>(
    &self,
    call: impl vox::Call<'vox, &'vox str, std::convert::Infallible>,
    word: String,
) {
    call.ok("short").await;
}
```

## Channel lifetime

Raw `Tx<T>`/`Rx<T>` channels are request-scoped sidebands. Start the call that
binds the channel, then drive channel send/receive work concurrently with that
call. The method response terminates the request scope, so channel data that
matters must be sent and drained before, or as part of, the response. Durable or
resumable streams belong in explicit service-level protocols, not raw channels.

For non-Rust bindings, generate code from service descriptors with `vox-codegen`.
