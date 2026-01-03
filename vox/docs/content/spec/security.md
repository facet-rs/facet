+++
title = "Security Profiles"
description = "Security requirements and deployment profiles"
weight = 95
+++

This document defines security profiles for Rapace deployments. Rapace does not mandate specific security mechanisms but defines normative requirements for different trust environments.

## Overview

Rapace is transport-agnostic and does not include built-in encryption or authentication. Security is delegated to:

- **Transport layer**: TLS, QUIC, Unix socket permissions, etc.
- **Application layer**: Tokens in Hello params or OpenChannel metadata

This design allows flexibility but requires explicit security consideration for each deployment.

## Security Profiles

### Profile A: Trusted Local

**Environment**: Same process, same trust domain, localhost-only communication.

Implementations may use no transport security in trusted local environments. See [Deployment Guide](/guide/deployment/#profile-a-trusted-local) for recommendations on Unix sockets and file permissions.

r[security.profile-a.multitenant]
Multi-tenant deployments MUST still authenticate and authorize at the application layer.

**Examples**:
- In-process service mesh sidecar
- Same-host microservices under single operator
- Development/testing environments

### Profile B: Same Host, Untrusted

**Environment**: Same machine, but different trust domains (e.g., plugins, multi-tenant workloads).

r[security.profile-b.authenticate]
Implementations MUST authenticate peers at the RPC layer (token in Hello params or per-call metadata).

r[security.profile-b.authorize]
Implementations MUST authorize each call based on the authenticated identity.

Implementations should use OS-level isolation and SHM transport with appropriate permissions. See [Deployment Guide](/guide/deployment/#profile-b-same-host-untrusted) for details.

**Examples**:
- Plugin system where plugins are untrusted
- Multi-tenant SaaS on shared infrastructure
- Sandboxed extensions

### Profile C: Networked / Untrusted

**Environment**: Communication over networks, potentially hostile environments.

r[security.profile-c.encryption]
Implementations MUST use confidentiality and integrity protection (TLS 1.3+, QUIC, WireGuard, etc.).

r[security.profile-c.authenticate]
Implementations MUST authenticate peers (mutual TLS, bearer tokens, etc.).

r[security.profile-c.reject]
Implementations MUST reject connections with invalid or missing authentication.

Implementations should use certificate pinning for high-security deployments. See [Deployment Guide](/guide/deployment/#profile-c-networked--untrusted) for details.

**Examples**:
- Microservices across data centers
- Client-server applications
- Public-facing APIs

## Authentication Mechanisms

### Hello Params Authentication

Authentication tokens can be passed in the `Hello.params` field during handshake:

```rust
Hello {
    params: vec![
        ("rapace.auth_token".into(), token_bytes),
        ("rapace.auth_scheme".into(), b"bearer".to_vec()),
    ],
    // ...
}
```

**Processing**:
1. Server extracts auth token from Hello params
2. Server validates token (JWT verification, database lookup, etc.)
3. If invalid: send `CloseChannel { reason: Error("authentication failed") }` and close
4. If valid: proceed with handshake, associate identity with connection

### Per-Call Authentication

For finer-grained access control, use `OpenChannel.metadata`:

```rust
OpenChannel {
    metadata: vec![
        ("rapace.auth_token".into(), call_specific_token),
    ],
    // ...
}
```

This allows:
- Different tokens per call (e.g., per-request OAuth tokens)
- Capability-based security (token encodes allowed operations)
- Token refresh without reconnecting

### Mutual TLS

For transport-level authentication:

1. Server presents certificate during TLS handshake
2. Client validates server certificate
3. Client presents certificate (mutual TLS)
4. Server validates client certificate
5. Rapace handshake proceeds over established TLS connection

The TLS identity can be associated with the Rapace connection for authorization decisions.

## Authentication Failure Behavior

### During Handshake

r[security.auth-failure.handshake]
If authentication fails during `Hello` exchange, the receiver MUST:

1. Send `CloseChannel { channel_id: 0, reason: Error("authentication failed") }`
2. Close the transport connection
3. Discard any other frames without processing

### During Call

If per-call authentication fails, the response depends on channel kind:

**For CALL channels**:
1. Server processes the request normally up to authentication check
2. Server responds with `CallResult { status: { code: UNAUTHENTICATED, message: "..." }, body: None }`
3. Connection remains open for other calls

**For STREAM/TUNNEL channels** (attached to a CALL):
1. Server sends `CancelChannel { channel_id, reason: Unauthenticated }` (or `PermissionDenied`)
2. The parent CALL fails with corresponding error code

This approach allows clients to distinguish auth failures from protocol errors and handle them appropriately (e.g., refresh tokens vs. report bugs).

### Error Codes

Use these error codes for authentication/authorization failures:

| Code | Name | Meaning |
|------|------|---------|
| 16 | `UNAUTHENTICATED` | No valid credentials provided |
| 7 | `PERMISSION_DENIED` | Valid credentials, but not authorized for this operation |

See [Error Handling](@/spec/errors.md) for the full error code list.

## Metadata Security

r[security.metadata.plaintext]
Implementations MUST be aware that Hello params and OpenChannel metadata are transmitted as plaintext in the Rapace payload (not encrypted by the protocol itself).

r[security.metadata.secrets]
Implementations MUST use transport encryption when transmitting sensitive data (passwords, long-lived secrets) in metadata.

Tokens in metadata should be short-lived and scoped. For sensitive operations, use transport-level security (TLS) as the foundation. See [Deployment Guide](/guide/deployment/#metadata-security) for details.

## Recommendations by Deployment

| Deployment | Transport | Auth | Notes |
|------------|-----------|------|-------|
| In-process | Direct call | N/A | No Rapace needed |
| Same-host trusted | Unix socket / SHM | Optional | Use file permissions |
| Same-host untrusted | SHM + token | Required | Validate on every call |
| LAN (trusted) | TCP + TLS optional | Token or mTLS | Defense in depth |
| WAN / Internet | TCP + TLS required | mTLS or token | Always encrypt |
| Browser | WebSocket + TLS | Token | Use WSS only |

## Security Checklist

For production deployments:

- [ ] Identify trust profile (A, B, or C)
- [ ] Configure appropriate transport security
- [ ] Implement authentication in Hello params or per-call
- [ ] Implement authorization checks on service methods
- [ ] Set appropriate timeouts and rate limits
- [ ] Log authentication failures
- [ ] Rotate secrets regularly

## Next Steps

- [Handshake & Capabilities](@/spec/handshake.md) - Where auth tokens are passed
- [Error Handling](@/spec/errors.md) - UNAUTHENTICATED and PERMISSION_DENIED codes
- [Metadata Conventions](@/spec/metadata.md) - Standard metadata keys
