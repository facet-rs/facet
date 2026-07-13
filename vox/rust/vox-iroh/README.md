# vox-iroh

`vox-iroh` maps one authenticated Iroh bidirectional QUIC stream onto the
ordinary bounded Vox stream link. Iroh supplies NAT traversal, relay fallback,
and cryptographically verified Ed25519 endpoint identities; Vox retains its
transport prologue, connection handshake, identity policy, lanes, requests,
flow control, and observability.

The transport's versioned ALPN is `vox/iroh/1`.

