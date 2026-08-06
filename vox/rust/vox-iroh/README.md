# vox-iroh

`vox-iroh` maps one authenticated Iroh bidirectional QUIC stream onto the
ordinary bounded Vox stream link. Iroh supplies NAT traversal, relay fallback,
and cryptographically verified Ed25519 endpoint identities; Vox retains its
transport prologue, connection handshake, identity policy, lanes, requests,
flow control, and observability.

The transport's versioned ALPN is `vox/iroh/1`.

## Browser WebAssembly

`vox-iroh` supports relay-only browser clients on `wasm32-unknown-unknown`.
`IrohLinkSource` connects to a remote endpoint through Iroh's browser transport;
the resulting `IrohLink` uses the same `vox/iroh/1` framing and authenticated
endpoint evidence as native clients.

`IrohListener` is native-only because browsers cannot accept incoming Iroh
connections. The underlying `vox-stream` crate keeps its generic
`AsyncRead`/`AsyncWrite` framing available on WebAssembly while its TCP, stdio,
and local-IPC constructors remain native-only.

