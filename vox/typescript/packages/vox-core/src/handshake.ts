import { hexToBytes } from "@bearcove/phon-schema";
import { buildPlan, decodeTyped, encodeTyped } from "@bearcove/phon-engine";
import {
  parseSchemaClosure,
  messageRegistry,
  messageSchemaClosure,
  messageSchemaId,
  type Metadata,
  emptyMetadata,
  coerceMetadata,
} from "@bearcove/vox-wire";
import type { ConnectionSettings, Parity } from "@bearcove/vox-wire";
import type { Link } from "./link.ts";
import {
  registry,
  schemaId,
  handshakeSchemaClosure,
  type Decline,
  type EstablishmentRejectReason as WireEstablishmentRejectReason,
  type HandshakeMessage,
} from "./handshake.phon.generated.ts";
import {
  observeEstablishmentFinished,
  observeEstablishmentStarted,
  type EstablishmentRole,
  type VoxObserver,
} from "./observer.ts";

// Re-export Metadata for downstream consumers that used to import it from here.
export type { Metadata } from "@bearcove/vox-wire";

export interface HandshakeResult {
  localSettings: ConnectionSettings;
  peerSettings: ConnectionSettings;
  peerMessageSchema: Uint8Array;
  peerMetadata: Metadata;
  peerEvidence: PeerEvidence;
  peerIdentity: PeerIdentity;
}

export const ESTABLISHMENT_REJECT_REASONS = [
  "unauthenticated",
  "forbidden",
  "not-ready",
  "draining",
  "unsupported",
  "policy-rejected",
] as const;
// r[impl rejection.reason.taxonomy]
// r[impl connection.policy.establishment.rejection]
export type EstablishmentRejectReason = (typeof ESTABLISHMENT_REJECT_REASONS)[number];

// r[impl connection.identity.use-cases]
// r[impl connection.identity.redaction]
export type PeerEvidenceItem =
  | { kind: "synthetic"; label: string }
  | { kind: "tls"; verifiedSubject?: string; alpn?: string }
  | { kind: "unix-peer-credentials"; uid?: number; gid?: number; pid?: number }
  | { kind: "platform-process"; description: string }
  | { kind: "xpc"; codeSigningIdentity: string }
  | { kind: "in-process"; component: string };

const peerEvidenceBrand: unique symbol = Symbol("vox.peerEvidence");

// r[impl connection.evidence]
export interface PeerEvidence {
  readonly [peerEvidenceBrand]: true;
  readonly items: readonly PeerEvidenceItem[];
}

export type PeerIdentityForm =
  | "anonymous"
  | "synthetic"
  | "local-process"
  | "certificate-backed"
  | "application-user"
  | "composite";

export type IdentityBasisProvenance =
  | "evidence-backed"
  | "verified-claim-backed"
  | "synthetic";

// r[impl connection.identity.forms]
export interface IdentityBasis {
  readonly form: PeerIdentityForm;
  readonly provenance: IdentityBasisProvenance;
  readonly redacted: string;
}

// r[impl connection.identity]
// r[impl connection.identity.late-claims]
// r[impl connection.identity.scope]
export interface PeerIdentity {
  readonly epoch: 0;
  readonly form: PeerIdentityForm;
  readonly bases: readonly IdentityBasis[];
}

// r[impl connection.identity.inputs]
// r[impl connection.identity.local]
export interface IdentityResolutionContext {
  readonly role: EstablishmentRole;
  readonly evidence: PeerEvidence;
  readonly claims: Metadata;
}

export interface IdentityDecline {
  readonly kind: "decline";
  readonly reason: EstablishmentRejectReason;
  readonly metadata?: Metadata;
}

export type IdentityResolution = PeerIdentity | IdentityDecline;
export type IdentityResolver =
  (context: IdentityResolutionContext) => IdentityResolution | Promise<IdentityResolution>;

export interface HandshakePolicyOptions {
  peerEvidence?: PeerEvidence;
  identityResolver?: IdentityResolver;
  observer?: VoxObserver;
}

export class ConnectionDeclinedError extends Error {
  readonly reason: EstablishmentRejectReason;
  readonly metadata: Metadata;
  readonly receivedFromPeer: boolean;

  constructor(
    reason: EstablishmentRejectReason,
    metadata: Metadata = emptyMetadata(),
    receivedFromPeer = false,
  ) {
    super(`connection establishment rejected: ${reason}`);
    this.name = "ConnectionDeclinedError";
    this.reason = reason;
    this.metadata = metadata;
    this.receivedFromPeer = receivedFromPeer;
  }
}

export function noPeerEvidence(): PeerEvidence {
  return { [peerEvidenceBrand]: true, items: [] };
}

export function syntheticPeerEvidence(label: string): PeerEvidence {
  return { [peerEvidenceBrand]: true, items: [{ kind: "synthetic", label }] };
}

export function anonymousPeerIdentity(): PeerIdentity {
  return { epoch: 0, form: "anonymous", bases: [] };
}

export function identityBasis(
  form: PeerIdentityForm,
  provenance: IdentityBasisProvenance,
  redacted: string,
): IdentityBasis {
  return { form, provenance, redacted };
}

export function peerIdentityFromBasis(basis: IdentityBasis): PeerIdentity {
  return { epoch: 0, form: basis.form, bases: [basis] };
}

export function compositePeerIdentity(bases: readonly IdentityBasis[]): PeerIdentity {
  return {
    epoch: 0,
    form: bases.length <= 1 ? bases[0]?.form ?? "anonymous" : "composite",
    bases: [...bases],
  };
}

// r[impl lane.authorization.context]
// r[impl request.authorization]
export interface LaneGrant {
  readonly metadata: Metadata;
}

export function emptyLaneGrant(): LaneGrant {
  return { metadata: emptyMetadata() };
}

// r[impl request.authorization]
export interface RequestAuthorizationContext {
  readonly peerIdentity: PeerIdentity;
  readonly peerEvidence: PeerEvidence;
  readonly laneGrant: LaneGrant;
}

export function requestAuthorizationContext(
  peerIdentity: PeerIdentity,
  peerEvidence: PeerEvidence,
  laneGrant: LaneGrant = emptyLaneGrant(),
): RequestAuthorizationContext {
  return { peerIdentity, peerEvidence, laneGrant };
}

export function anonymousRequestAuthorizationContext(): RequestAuthorizationContext {
  return requestAuthorizationContext(
    anonymousPeerIdentity(),
    noPeerEvidence(),
    emptyLaneGrant(),
  );
}

export function declineIdentity(
  reason: EstablishmentRejectReason,
  metadata: Metadata = emptyMetadata(),
): IdentityDecline {
  return { kind: "decline", reason, metadata };
}

function isIdentityDecline(value: IdentityResolution): value is IdentityDecline {
  return "kind" in value && value.kind === "decline";
}

function wireRejectReason(reason: EstablishmentRejectReason): WireEstablishmentRejectReason {
  switch (reason) {
    case "unauthenticated":
      return { tag: "Unauthenticated" };
    case "forbidden":
      return { tag: "Forbidden" };
    case "not-ready":
      return { tag: "NotReady" };
    case "draining":
      return { tag: "Draining" };
    case "unsupported":
      return { tag: "Unsupported" };
    case "policy-rejected":
      return { tag: "PolicyRejected" };
  }
}

function rejectReasonFromWire(reason: WireEstablishmentRejectReason): EstablishmentRejectReason {
  switch (reason.tag) {
    case "Unauthenticated":
      return "unauthenticated";
    case "Forbidden":
      return "forbidden";
    case "NotReady":
      return "not-ready";
    case "Draining":
      return "draining";
    case "Unsupported":
      return "unsupported";
    case "PolicyRejected":
      return "policy-rejected";
  }
}

function declineErrorFromWire(decline: Decline): ConnectionDeclinedError {
  return new ConnectionDeclinedError(
    rejectReasonFromWire(decline.reason),
    coerceMetadata(decline.metadata),
    true,
  );
}

// ---------------------------------------------------------------------------
// phon self-describing framing
//
// Each handshake message is sent as:
//   [u32 schema_len little-endian][schema-closure bytes][phon-compact value]
// ---------------------------------------------------------------------------

function encodeHandshake(msg: HandshakeMessage): Uint8Array {
  const value = encodeTyped(msg as never, schemaId.HandshakeMessage, registry);
  const closure = hexToBytes(handshakeSchemaClosure);

  const out = new Uint8Array(4 + closure.length + value.length);
  const dv = new DataView(out.buffer, out.byteOffset, out.byteLength);
  dv.setUint32(0, closure.length, true);
  out.set(closure, 4);
  out.set(value, 4 + closure.length);
  return out;
}

function decodeHandshake(bytes: Uint8Array): HandshakeMessage {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const len = dv.getUint32(0, true);
  const closure = bytes.subarray(4, 4 + len);
  const value = bytes.subarray(4 + len);
  const { root, schemas } = parseSchemaClosure(closure);
  return decodeTyped(
    value,
    root,
    schemaId.HandshakeMessage,
    registry.with(schemas),
  ) as unknown as HandshakeMessage;
}

async function recvHandshake(link: Link): Promise<HandshakeMessage> {
  const payload = await link.recv();
  if (!payload) {
    throw new Error("peer closed during handshake");
  }
  return decodeHandshake(payload);
}

async function sendHandshake(link: Link, message: HandshakeMessage): Promise<void> {
  await link.send(encodeHandshake(message));
}

const UNSUPPORTED_MESSAGE_COMPATIBILITY_PLAN = "unsupported message compatibility plan";

function peerMessageSchemaRejectionReason(peerSchema: Uint8Array): string | null {
  try {
    const { root, schemas } = parseSchemaClosure(peerSchema);
    buildPlan(root, messageSchemaId.Message, messageRegistry.with(schemas));
    return null;
  } catch {
    return UNSUPPORTED_MESSAGE_COMPATIBILITY_PLAN;
  }
}

async function sendSorryAndReject(link: Link, reason: string): Promise<never> {
  await sendHandshake(link, { tag: "Sorry", value: { reason } });
  throw new Error(reason);
}

async function sendDeclineAndReject(
  link: Link,
  reason: EstablishmentRejectReason,
  metadata: Metadata = emptyMetadata(),
): Promise<never> {
  // r[impl connection.handshake.decline]
  // r[impl connection.policy.establishment.rejection]
  // r[impl rpc.metadata.records]
  await sendHandshake(link, {
    tag: "Decline",
    value: {
      reason: wireRejectReason(reason),
      metadata,
    },
  });
  throw new ConnectionDeclinedError(reason, metadata);
}

// r[impl connection.identity.resolver]
// r[impl connection.policy.establishment]
async function resolvePeerIdentity(
  role: EstablishmentRole,
  claims: Metadata,
  link: Link,
  options: HandshakePolicyOptions,
): Promise<PeerIdentity> {
  const evidence = options.peerEvidence ?? noPeerEvidence();
  const resolver = options.identityResolver ??
    (() => anonymousPeerIdentity());
  const identityContext = { role, phase: "identity-resolution" as const };
  const policyContext = { role, phase: "connection-policy" as const };
  const identityStartedAt = observeEstablishmentStarted(options.observer, identityContext);
  const policyStartedAt = observeEstablishmentStarted(options.observer, policyContext);

  try {
    const result = await resolver({ role, evidence, claims });
    if (isIdentityDecline(result)) {
      await sendDeclineAndReject(
        link,
        result.reason,
        result.metadata ?? emptyMetadata(),
      );
      throw new Error("unreachable after sending Decline");
    }
    const identity: PeerIdentity = result;
    observeEstablishmentFinished(
      options.observer,
      identityContext,
      identityStartedAt,
      "ok",
    );
    observeEstablishmentFinished(
      options.observer,
      policyContext,
      policyStartedAt,
      "ok",
    );
    return identity;
  } catch (error) {
    const outcome = error instanceof ConnectionDeclinedError ? "rejected" : "error";
    observeEstablishmentFinished(
      options.observer,
      identityContext,
      identityStartedAt,
      outcome,
      error,
    );
    observeEstablishmentFinished(
      options.observer,
      policyContext,
      policyStartedAt,
      outcome,
      error,
    );
    throw error;
  }
}

function oppositeParity(parity: Parity): Parity {
  return parity.tag === "Odd" ? { tag: "Even" } : { tag: "Odd" };
}

// The sender's Message-envelope schema closure, sent verbatim as a byte list.
function localMessagePayloadSchema(): number[] {
  return Array.from(hexToBytes(messageSchemaClosure));
}

export async function handshakeAsInitiator(
  link: Link,
  settings: ConnectionSettings,
  metadata: Metadata = emptyMetadata(),
  policy: HandshakePolicyOptions = {},
): Promise<HandshakeResult> {
  // r[impl connection.handshake.metadata]
  // r[impl rpc.metadata.records]
  await sendHandshake(link, {
    tag: "Hello",
    value: {
      parity: settings.parity,
      connection_settings: settings,
      message_payload_schema: localMessagePayloadSchema(),
      metadata,
    },
  });

  const response = await recvHandshake(link);
  if (response.tag === "Decline") {
    throw declineErrorFromWire(response.value);
  }
  if (response.tag === "Sorry") {
    throw new Error(`handshake rejected: ${response.value.reason}`);
  }
  if (response.tag !== "HelloYourself") {
    throw new Error("expected HelloYourself, Decline, or Sorry during handshake");
  }

  const helloYourself = response;
  const peerMetadata = coerceMetadata(helloYourself.value.metadata);
  const peerMessageSchema = new Uint8Array(response.value.message_payload_schema);
  if (helloYourself.value.connection_settings.initial_channel_credit <= 0) {
    await sendSorryAndReject(link, "initial_channel_credit must be greater than zero");
  }
  const peerIdentity = await resolvePeerIdentity("initiator", peerMetadata, link, policy);
  const rejectionReason = peerMessageSchemaRejectionReason(peerMessageSchema);
  if (rejectionReason !== null) {
    await sendSorryAndReject(link, rejectionReason);
  }

  await sendHandshake(link, { tag: "LetsGo", value: {} });

  return {
    localSettings: settings,
    peerSettings: helloYourself.value.connection_settings,
    peerMessageSchema,
    peerMetadata,
    peerEvidence: policy.peerEvidence ?? noPeerEvidence(),
    peerIdentity,
  };
}

export async function handshakeAsAcceptor(
  link: Link,
  settings: ConnectionSettings,
  metadata: Metadata = emptyMetadata(),
  policy: HandshakePolicyOptions = {},
): Promise<HandshakeResult> {
  // r[impl connection.handshake.metadata]
  // r[impl rpc.metadata.records]
  const first = await recvHandshake(link);
  if (first.tag !== "Hello") {
    throw new Error("expected Hello during handshake");
  }
  const hello = first;
  const peerMetadata = coerceMetadata(hello.value.metadata);
  const peerMessageSchema = new Uint8Array(hello.value.message_payload_schema);
  if (hello.value.connection_settings.initial_channel_credit <= 0) {
    await sendSorryAndReject(link, "initial_channel_credit must be greater than zero");
  }
  const peerIdentity = await resolvePeerIdentity("acceptor", peerMetadata, link, policy);
  const rejectionReason = peerMessageSchemaRejectionReason(peerMessageSchema);
  if (rejectionReason !== null) {
    await sendSorryAndReject(link, rejectionReason);
  }
  const localSettings = {
    ...settings,
    parity: oppositeParity(hello.value.parity),
  };

  await sendHandshake(link, {
    tag: "HelloYourself",
    value: {
      connection_settings: localSettings,
      message_payload_schema: localMessagePayloadSchema(),
      metadata,
    },
  });

  const third = await recvHandshake(link);
  if (third.tag === "Decline") {
    throw declineErrorFromWire(third.value);
  }
  if (third.tag === "Sorry") {
    throw new Error(`handshake rejected: ${third.value.reason}`);
  }
  if (third.tag !== "LetsGo") {
    throw new Error("expected LetsGo, Decline, or Sorry during handshake");
  }

  return {
    localSettings,
    peerSettings: hello.value.connection_settings,
    peerMessageSchema,
    peerMetadata,
    peerEvidence: policy.peerEvidence ?? noPeerEvidence(),
    peerIdentity,
  };
}

export function voxServiceMetadata(serviceName: string): Metadata {
  const metadata: Metadata = new Map();
  metadata.set("vox-service", serviceName);
  return metadata;
}
