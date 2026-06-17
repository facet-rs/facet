import { encodeTyped } from "@bearcove/phon-engine";
import {
  type ConnectionSettings,
  type RequestMessage,
  type ChannelMessage,
  type SchemaMessage,
  type LaneReject,
  type Message,
  type Metadata,
  emptyMetadata,
  coerceMetadata,
  messageLaneAccept,
  messageLaneOpen,
  messageLaneClose,
  messageLaneReject,
  messageProtocolError,
  messageRequest,
  messageResponse,
  messageSchema,
  messageCancel,
  messageData,
  messageClose,
  messageCredit,
  RpcError,
  RpcErrorCode,
} from "@bearcove/vox-wire";
import {
  ChannelError,
  ChannelIdAllocator,
  ChannelRegistry,
  DEFAULT_INITIAL_CREDIT,
  Role,
  bindPhonChannels,
  type ChannelRegistryDebugSnapshot,
  type ServiceDescriptor,
} from "./channeling/index.ts";
import type { Caller, CallerRequest } from "./caller.ts";
import { MiddlewareCaller } from "./caller.ts";
import type { ClientMiddleware } from "./middleware.ts";
import { ClientMetadata } from "./metadata.ts";
import type { Conduit } from "./conduit.ts";
import { AsyncSemaphore } from "./internal/async_semaphore.ts";
import { AsyncQueue } from "./internal/async_queue.ts";
import { deferred, type Deferred } from "./internal/deferred.ts";
import {
  firstIdForParity,
  oppositeParity,
  parityFromRole,
  roleFromParity,
} from "./internal/parity.ts";
import { BareConduit, buildMessageDecodePlan } from "./conduit.ts";
import { voxLogger } from "./logger.ts";
import { SchemaCompatibilityError, SchemaTracker, SchemaSendTracker } from "./schema_tracker.ts";
import type { Link, LinkSource } from "./link.ts";
import {
  performAcceptorTransportPrologue,
  performInitiatorTransportPrologue,
} from "./transport_prologue.ts";
import {
  ConnectionDeclinedError,
  emptyLaneGrant,
  handshakeAsAcceptor,
  handshakeAsInitiator,
  type HandshakeResult,
  type IdentityResolver,
  type LaneGrant,
  type PeerEvidence,
  type PeerIdentity,
  requestAuthorizationContext,
  type RequestAuthorizationContext,
  voxServiceMetadata,
} from "./handshake.ts";
import {
  observeEstablishmentFinished,
  observeEstablishmentStarted,
  splitQualifiedMethodName,
  type EstablishmentContext,
  type EstablishmentRole,
  type VoxObserver,
} from "./observer.ts";

const DEFAULT_TIMEOUT_MS = 30_000;
export const VOX_LANE_REJECT_REASON_METADATA_KEY = "vox-lane-reject-reason";
export const VOX_LANE_REJECT_MESSAGE_METADATA_KEY = "vox-lane-reject-message";
export const LANE_REJECT_REASONS = [
  "unknown-service",
  "forbidden",
  "not-ready",
  "draining",
  "schema-incompatible",
  "policy-rejected",
] as const;
// r[impl lane.authorization]
// r[impl lane.authorization.filtered]
// r[impl rejection.reason.taxonomy]
export type LaneRejectReason = (typeof LANE_REJECT_REASONS)[number];

function isLaneRejectReason(value: unknown): value is LaneRejectReason {
  return typeof value === "string" &&
    (LANE_REJECT_REASONS as readonly string[]).includes(value);
}

function metadataString(metadata: Metadata, key: string): string | undefined {
  const value = metadata.get(key);
  return typeof value === "string" ? value : undefined;
}

function laneRejectionMessage(
  reason: LaneRejectReason,
  message: string | undefined,
): string {
  return message
    ? `lane open rejected: ${reason}: ${message}`
    : `lane open rejected: ${reason}`;
}

// r[impl lane.open.result]
// r[impl lane.authorization]
// r[impl lane.authorization.filtered]
export class LaneRejection {
  readonly reason: LaneRejectReason;
  readonly metadata: Metadata;

  private constructor(reason: LaneRejectReason, metadata: Metadata) {
    this.reason = reason;
    this.metadata = metadata;
  }

  static new(reason: LaneRejectReason): LaneRejection {
    return LaneRejection.withMetadata(reason);
  }

  static withMessage(reason: LaneRejectReason, message: string): LaneRejection {
    const metadata = emptyMetadata();
    metadata.set(VOX_LANE_REJECT_MESSAGE_METADATA_KEY, message);
    return LaneRejection.withMetadata(reason, metadata);
  }

  static withMetadata(
    reason: LaneRejectReason,
    metadata: Metadata = emptyMetadata(),
  ): LaneRejection {
    const next = new Map(metadata);
    next.set(VOX_LANE_REJECT_REASON_METADATA_KEY, reason);
    return new LaneRejection(reason, next);
  }

  static fromMetadata(metadata: Metadata): LaneRejection {
    const value = metadata.get(VOX_LANE_REJECT_REASON_METADATA_KEY);
    const reason = isLaneRejectReason(value) ? value : "policy-rejected";
    return LaneRejection.withMetadata(reason, metadata);
  }

  message(): string | undefined {
    return metadataString(this.metadata, VOX_LANE_REJECT_MESSAGE_METADATA_KEY) ??
      metadataString(this.metadata, "error");
  }

  toMetadata(): Metadata {
    return new Map(this.metadata);
  }
}

// r[impl lane.authorization]
// r[impl lane.authorization.context]
export interface LaneRequest {
  readonly metadata: Metadata;
  readonly service: string;
  readonly peerIdentity: PeerIdentity;
  readonly peerEvidence: PeerEvidence;
}

// r[impl lane.accept.api]
export type LaneAcceptor =
  (request: LaneRequest, lane: PendingLane) => void | Promise<void>;

// r[impl lane.accept.api]
// r[impl lane.authorization.context]
export class PendingLane {
  private handled = false;

  constructor(
    readonly id: bigint,
    private readonly acceptFn: (grant: LaneGrant) => Promise<Lane>,
    private readonly rejectFn: (rejection: LaneRejection) => Promise<void>,
  ) {}

  isHandled(): boolean {
    return this.handled;
  }

  async accept(grant: LaneGrant = emptyLaneGrant()): Promise<Lane> {
    if (this.handled) {
      throw new Error("PendingLane already handled");
    }
    this.handled = true;
    return await this.acceptFn(grant);
  }

  async reject(rejection: LaneRejection = LaneRejection.new("policy-rejected")): Promise<void> {
    if (this.handled) {
      throw new Error("PendingLane already handled");
    }
    this.handled = true;
    await this.rejectFn(rejection);
  }
}

interface PendingResponse {
  resolve: (payload: Uint8Array) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
  idleTimeoutMs: number;
  lastProgressAt: number;
  methodId: bigint;
  payload: Uint8Array;
  metadata: Metadata;
  channels: bigint[];
  /**
   * Lazily computes args schemas bytes for each send attempt.
   * Called when sending so the current connection's schema tracker can decide
   * whether fresh schemas are required.
   */
  computeSchemas?: () => number[];
  finalizeChannels?: () => void;
  requestIds: Set<bigint>;
  settled: boolean;
}

export interface IncomingCall {
  // r[impl request.authorization]
  requestId: bigint;
  methodId: bigint;
  args: Uint8Array;
  channels: bigint[];
  metadata: Metadata;
  laneEpoch: number;
  authorization: RequestAuthorizationContext;
}

export interface LaneDebugSnapshot {
  laneId: bigint;
  closed: boolean;
  pendingResponseCount: number;
  pendingRequestIds: bigint[];
  inboundLiveRequestCount: number;
  inboundLiveRequestIds: bigint[];
  flowControl: {
    localMaxConcurrentRequests: number;
    peerMaxConcurrentRequests: number;
    outboundRequestLimit: {
      availablePermits: number;
      waitingCount: number;
      closed: boolean;
    };
  };
  channels: ChannelRegistryDebugSnapshot;
}

export interface ConnectionBuilderOptions {
  maxConcurrentRequests?: number;
  channelCapacity?: number;
  metadata?: Metadata;
  identityResolver?: IdentityResolver;
  onLane?: LaneAcceptor;
  observer?: VoxObserver;
  /**
   * If set, the connection sends a Ping every `keepaliveIntervalMs` milliseconds
   * and expects a Pong back within `keepaliveTimeoutMs` (default: half the
   * interval). If no Pong arrives in time the connection closes. Set to 0 or
   * undefined to disable keepalive.
   *
   * Recommended for WebSocket lanes where silent network drops are
   * common (proxies, mobile networks, etc.) and the underlying transport
   * may not surface a close/error event promptly.
   */
  keepaliveIntervalMs?: number;
  /**
   * How long (ms) to wait for a Pong before declaring the connection dead.
   * Defaults to half of `keepaliveIntervalMs` when not specified.
   */
  keepaliveTimeoutMs?: number;
}

export interface ConnectionTransportOptions extends ConnectionBuilderOptions {
}

export interface LaneOpenOptions {
  settings?: ConnectionSettings;
  metadata?: Metadata;
}

export interface LaneClientConstructor<T> {
  new(caller: Caller): T;
  descriptor?: ServiceDescriptor;
}

export interface ConnectLaneOptions extends ConnectionTransportOptions {
  laneSettings?: ConnectionSettings;
  laneMetadata?: Metadata;
}

const DEFAULT_CHANNEL_CAPACITY = 16;

function channelCapacityFromOptions(options: ConnectionBuilderOptions): number {
  const channelCapacity = options.channelCapacity ?? DEFAULT_CHANNEL_CAPACITY;
  // r[impl rpc.flow-control.credit.initial.high-level]
  // r[impl rpc.flow-control.credit.initial.zero]
  if (channelCapacity <= 0) {
    throw ConnectionError.protocol("initial_channel_credit must be greater than zero");
  }
  return channelCapacity;
}

type ConnectionTransport = Link | LinkSource;

function isLinkSource(value: ConnectionTransport): value is LinkSource {
  return typeof (value as LinkSource).nextLink === "function";
}

function cloneMetadata(metadata: Metadata): Metadata {
  return new Map(metadata);
}

interface EstablishedTransport {
  conduit: Conduit<Message>;
  handshake: HandshakeResult;
}

async function observeEstablishment<T>(
  observer: VoxObserver | undefined,
  context: EstablishmentContext,
  body: () => Promise<T> | T,
): Promise<T> {
  const startedAt = observeEstablishmentStarted(observer, context);
  try {
    const result = await body();
    observeEstablishmentFinished(observer, context, startedAt, "ok");
    return result;
  } catch (error) {
    observeEstablishmentFinished(
      observer,
      context,
      startedAt,
      error instanceof ConnectionDeclinedError ? "rejected" : "error",
      error,
    );
    throw error;
  }
}

function roleName(role: Role): EstablishmentRole {
  return role === Role.Initiator ? "initiator" : "acceptor";
}

// r[impl connection.protocol]
// r[impl connection.handshake]
// r[impl connection.handshake.phon]
// r[impl connection.handshake.protocol-schema]
// r[impl connection.handshake.protocol-schema.connection-scoped]
// r[impl connection.handshake.unversioned]
// r[impl lane.settings]
// r[impl connection.handshake.lane-settings]
// r[impl connection.peer]
// r[impl connection.role]
// r[impl connection.symmetry]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.post-accept]
async function makeInitiatorEstablishedTransport(
  transport: ConnectionTransport,
  options: ConnectionTransportOptions,
): Promise<EstablishedTransport> {
  const observer = options.observer;
  const localSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    // r[impl rpc.flow-control.max-concurrent-requests.default]
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: channelCapacityFromOptions(options),
  };

  if (isLinkSource(transport)) {
    const attachment = await transport.nextLink();
    const peerEvidence = attachment.peerEvidence;
    await observeEstablishment(
      observer,
      { role: "initiator", phase: "transport-prologue" },
      () => performInitiatorTransportPrologue(attachment.link),
    );
    const handshake = await observeEstablishment(
      observer,
      { role: "initiator", phase: "connection-handshake" },
      () => handshakeAsInitiator(
        attachment.link,
        localSettings,
        options.metadata ?? emptyMetadata(),
        {
          peerEvidence,
          identityResolver: options.identityResolver,
          observer,
        },
      ),
    );
    const messagePlan = await observeEstablishment(
      observer,
      { role: "initiator", phase: "schema-decode-plan" },
      () => buildMessageDecodePlan(handshake.peerMessageSchema),
    );

    return {
      conduit: new BareConduit(attachment.link, messagePlan),
      handshake,
    };
  }

  await observeEstablishment(
    observer,
    { role: "initiator", phase: "transport-prologue" },
    () => performInitiatorTransportPrologue(transport),
  );
  const handshake = await observeEstablishment(
    observer,
    { role: "initiator", phase: "connection-handshake" },
    () => handshakeAsInitiator(
      transport,
      localSettings,
      options.metadata ?? emptyMetadata(),
      {
        identityResolver: options.identityResolver,
        observer,
      },
    ),
  );
  const messagePlan = await observeEstablishment(
    observer,
    { role: "initiator", phase: "schema-decode-plan" },
    () => buildMessageDecodePlan(handshake.peerMessageSchema),
  );

  return {
    conduit: new BareConduit(transport, messagePlan),
    handshake,
  };
}

// r[impl connection.protocol]
// r[impl connection.handshake]
// r[impl connection.handshake.phon]
// r[impl connection.handshake.protocol-schema]
// r[impl connection.handshake.protocol-schema.connection-scoped]
// r[impl connection.handshake.sorry]
// r[impl connection.handshake.unversioned]
// r[impl lane.settings]
// r[impl connection.handshake.lane-settings]
// r[impl connection.peer]
// r[impl connection.role]
// r[impl connection.symmetry]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.post-accept]
async function makeAcceptorEstablishedTransport(
  transport: ConnectionTransport,
  options: ConnectionTransportOptions,
): Promise<EstablishedTransport> {
  const observer = options.observer;
  const attachment = isLinkSource(transport)
    ? await transport.nextLink()
    : { link: transport };
  const peerEvidence = attachment.peerEvidence;
  await observeEstablishment(
    observer,
    { role: "acceptor", phase: "transport-prologue" },
    () => performAcceptorTransportPrologue(attachment.link),
  );

  const localSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    // r[impl rpc.flow-control.max-concurrent-requests.default]
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: channelCapacityFromOptions(options),
  };

  const handshake = await observeEstablishment(
    observer,
    { role: "acceptor", phase: "connection-handshake" },
    () => handshakeAsAcceptor(
      attachment.link,
      localSettings,
      options.metadata ?? emptyMetadata(),
      {
        peerEvidence,
        identityResolver: options.identityResolver,
        observer,
      },
    ),
  );
  const messagePlan = await observeEstablishment(
    observer,
    { role: "acceptor", phase: "schema-decode-plan" },
    () => buildMessageDecodePlan(handshake.peerMessageSchema),
  );

  return {
    conduit: new BareConduit(attachment.link, messagePlan),
    handshake,
  };
}

export class ConnectionError extends Error {
  readonly isProtocolError: boolean;
  readonly receivedFromPeer: boolean;
  readonly rejection?: LaneRejection;

  constructor(
    message: string,
    options: {
      isProtocolError?: boolean;
      receivedFromPeer?: boolean;
      rejection?: LaneRejection;
    } = {},
  ) {
    super(message);
    this.name = "ConnectionError";
    this.isProtocolError = options.isProtocolError ?? false;
    this.receivedFromPeer = options.receivedFromPeer ?? false;
    this.rejection = options.rejection;
  }

  static closed(): ConnectionError {
    return new ConnectionError("connection closed");
  }

  static protocol(message: string): ConnectionError {
    return new ConnectionError(message, { isProtocolError: true });
  }

  static peerProtocol(message: string): ConnectionError {
    return new ConnectionError(message, { isProtocolError: true, receivedFromPeer: true });
  }

  static rejected(rejection: LaneRejection): ConnectionError {
    return new ConnectionError(
      laneRejectionMessage(rejection.reason, rejection.message()),
      { rejection },
    );
  }
}

class ConnectionCore {
  private conduit: Conduit<Message>;
  private readonly lanes = new Map<bigint, Lane>();
  private readonly pendingLanes = new Map<
    bigint,
    {
      localSettings: ConnectionSettings;
      result: Deferred<Lane>;
      establishmentContext: EstablishmentContext;
      establishmentStartedAt: number;
    }
  >();
  private readonly connectionHandle: ConnectionHandle;
  private sendChain: Promise<void> = Promise.resolve();
  private nextLaneId: bigint;
  private closed = false;
  private closeError: ConnectionError | null = null;
  private initialLaneValue: Lane | null = null;
  private runPromise: Promise<void> | null = null;
  private readonly keepaliveIntervalMs: number;
  private readonly keepaliveTimeoutMs: number;
  private keepaliveTimer: ReturnType<typeof setTimeout> | null = null;
  private keepalivePendingNonce: bigint | null = null;
  private keepalivePongTimer: ReturnType<typeof setTimeout> | null = null;
  private nextKeepaliveNonce = 1n;
  private readonly localInitialLaneSettings: ConnectionSettings;
  private readonly peerInitialLaneSettings: ConnectionSettings;
  private readonly peerIdentity: PeerIdentity;
  private readonly peerEvidence: PeerEvidence;
  private readonly onLane?: LaneAcceptor;
  private readonly observer?: VoxObserver;

  constructor(
    conduit: Conduit<Message>,
    localInitialLaneSettings: ConnectionSettings,
    peerInitialLaneSettings: ConnectionSettings,
    peerIdentity: PeerIdentity,
    peerEvidence: PeerEvidence,
    onLane?: LaneAcceptor,
    keepaliveIntervalMs = 0,
    keepaliveTimeoutMs = 0,
    observer?: VoxObserver,
  ) {
    this.conduit = conduit;
    this.localInitialLaneSettings = localInitialLaneSettings;
    this.peerInitialLaneSettings = peerInitialLaneSettings;
    this.peerIdentity = peerIdentity;
    this.peerEvidence = peerEvidence;
    // r[impl connection.lane-id-parity]
    this.nextLaneId = firstIdForParity(localInitialLaneSettings.parity);
    this.connectionHandle = new ConnectionHandle(this);
    this.onLane = onLane;
    this.observer = observer;
    this.keepaliveIntervalMs = keepaliveIntervalMs;
    this.keepaliveTimeoutMs =
      keepaliveTimeoutMs > 0 ? keepaliveTimeoutMs : Math.floor(keepaliveIntervalMs / 2);
  }

  connectionHandleValue(): ConnectionHandle {
    return this.connectionHandle;
  }

  peerIdentityValue(): PeerIdentity {
    // r[impl connection.identity]
    // r[impl connection.identity.scope]
    return this.peerIdentity;
  }

  peerEvidenceValue(): PeerEvidence {
    // r[impl connection.evidence]
    return this.peerEvidence;
  }

  defaultLaneSettings(): ConnectionSettings {
    return {
      parity: this.localInitialLaneSettings.parity,
      max_concurrent_requests: this.localInitialLaneSettings.max_concurrent_requests,
      initial_channel_credit: this.localInitialLaneSettings.initial_channel_credit,
    };
  }

  initialLane(): Lane {
    // r[impl lane.id.compat]
    // r[impl lane.control.compat]
    // r[impl lane.control]
    if (!this.initialLaneValue) {
      this.initialLaneValue = new Lane(
        this,
        0n,
        this.localInitialLaneSettings,
        this.peerInitialLaneSettings,
      );
      this.lanes.set(0n, this.initialLaneValue);
    }
    return this.initialLaneValue;
  }

  failPendingAttempts(error: Error): void {
    for (const lane of this.lanes.values()) {
      lane.failPendingAttempts(error);
    }
  }

  start(): void {
    // r[impl connection.lifecycle.driven]
    // r[impl rpc.caller.liveness.explicit-shutdown-required]
    if (this.runPromise) {
      return;
    }
    this.runPromise = this.run().catch(async (error) => {
      voxLogger()?.error(`[vox:connection] run loop error:`, error);
      const connectionError = error instanceof ConnectionError
        ? error
        : new ConnectionError(String(error));
      if (
        connectionError.isProtocolError &&
        !connectionError.receivedFromPeer &&
        !this.closed
      ) {
        await this.sendProtocolError(connectionError.message);
      }
      this.fail(connectionError);
    });
    // r[impl connection.keepalive]
    if (this.keepaliveIntervalMs > 0) {
      this.scheduleKeepalive();
    }
  }

  // r[impl connection.keepalive]
  private scheduleKeepalive(): void {
    if (this.closed || this.keepaliveIntervalMs <= 0) return;
    this.keepaliveTimer = setTimeout(() => {
      this.sendKeepalivePing();
    }, this.keepaliveIntervalMs);
  }

  // r[impl connection.keepalive]
  private sendKeepalivePing(): void {
    if (this.closed) {
      this.scheduleKeepalive();
      return;
    }
    const nonce = this.nextKeepaliveNonce++;
    this.keepalivePendingNonce = nonce;
    void this.sendMessage({ lane_id: 0n, payload: { tag: "Ping", value: { nonce } } }).catch(() => {});

    // Expect a Pong within keepaliveTimeoutMs.
    this.keepalivePongTimer = setTimeout(() => {
      if (this.keepalivePendingNonce === nonce && !this.closed) {
        voxLogger()?.debug(
          `[vox:connection] keepalive timeout - no Pong received, treating as dead connection`,
        );
        this.keepalivePendingNonce = null;
        this.conduit.close();
      }
    }, this.keepaliveTimeoutMs);
  }

  private clearKeepaliveTimers(): void {
    if (this.keepaliveTimer !== null) {
      clearTimeout(this.keepaliveTimer);
      this.keepaliveTimer = null;
    }
    if (this.keepalivePongTimer !== null) {
      clearTimeout(this.keepalivePongTimer);
      this.keepalivePongTimer = null;
    }
    this.keepalivePendingNonce = null;
  }

  closedPromise(): Promise<void> {
    return this.runPromise ?? Promise.resolve();
  }

  async openLane(
    settings: ConnectionSettings,
    metadata: Metadata = emptyMetadata(),
  ): Promise<Lane> {
    // r[impl lane.id.compat]
    // r[impl lane.service.compat]
    // r[impl lane.open.wire]
    // r[impl lane.open]
    // r[impl lane.open.api]
    this.assertOpen();
    if (settings.initial_channel_credit <= 0) {
      throw ConnectionError.protocol("initial_channel_credit must be greater than zero");
    }

    const laneId = this.allocateLaneId();
    const result = deferred<Lane>();
    const establishmentContext: EstablishmentContext = {
      role: roleName(roleFromParity(this.localInitialLaneSettings.parity)),
      phase: "service-lane-open",
      laneId,
    };
    const establishmentStartedAt = observeEstablishmentStarted(
      this.observer,
      establishmentContext,
    );
    this.pendingLanes.set(laneId, {
      localSettings: settings,
      result,
      establishmentContext,
      establishmentStartedAt,
    });

    try {
      await this.sendMessage(messageLaneOpen(laneId, settings, metadata));
    } catch (error) {
      this.pendingLanes.delete(laneId);
      observeEstablishmentFinished(
        this.observer,
        establishmentContext,
        establishmentStartedAt,
        "error",
        error,
      );
      throw error;
    }

    return result.promise;
  }

  async closeLane(laneId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    // r[impl lane.id.compat]
    // r[impl lane.service.compat]
    // r[impl lane.close]
    // r[impl lane.close.semantics]
    this.assertOpen();
    if (laneId === 0n) {
      throw new ConnectionError("cannot close the initial lane through closeLane");
    }

    const lane = this.lanes.get(laneId);
    if (!lane) {
      throw ConnectionError.protocol(`unknown lane ${laneId}`);
    }

    this.observeLaneGrantRevocation(laneId, lane);
    lane.close(ConnectionError.closed());
    this.lanes.delete(laneId);
    await this.sendMessage(messageLaneClose(laneId, metadata));
  }

  async sendMessage(message: Message): Promise<void> {
    this.assertOpen();

    voxLogger()?.debug(
      `[vox:connection] sendMessage: tag=${message.payload.tag} lane=${message.lane_id}`,
    );
    const op = this.sendChain.then(() => this.conduit.send(message));
    this.sendChain = op.then(() => undefined, () => undefined);
    await op;
  }

  fail(error: ConnectionError): void {
    if (this.closed) {
      return;
    }

    this.clearKeepaliveTimers();
    this.closed = true;
    this.closeError = error;
    this.conduit.close();

    for (const pending of this.pendingLanes.values()) {
      observeEstablishmentFinished(
        this.observer,
        pending.establishmentContext,
        pending.establishmentStartedAt,
        "error",
        error,
      );
      pending.result.reject(error);
    }
    this.pendingLanes.clear();

    for (const [laneId, lane] of this.lanes) {
      this.observeLaneGrantRevocation(laneId, lane);
      lane.close(error);
    }
    this.lanes.clear();
  }

  shutdown(): void {
    // r[impl connection.shutdown.explicit]
    this.fail(ConnectionError.closed());
  }

  private assertOpen(): void {
    if (this.closed) {
      throw this.closeError ?? ConnectionError.closed();
    }
  }

  private allocateLaneId(): bigint {
    // r[impl connection.lane-id-parity]
    const id = this.nextLaneId;
    this.nextLaneId += 2n;
    return id;
  }

  private getLane(laneId: bigint): Lane {
    const lane = this.lanes.get(laneId);
    if (!lane) {
      throw ConnectionError.protocol(`unknown lane ${laneId}`);
    }
    return lane;
  }

  private async run(): Promise<void> {
    // r[impl connection.message]
    // r[impl connection.message.lane-id]
    // r[impl connection.message.payloads]
    while (!this.closed) {
      const message = await this.conduit.recv();
      if (!message) {
        this.clearKeepaliveTimers();
        this.failPendingAttempts(new ConnectionError("connection lost"));
        throw ConnectionError.closed();
      }
      await this.handleMessage(message);
    }
  }

  private async handleMessage(message: Message): Promise<void> {
    voxLogger()?.debug(`[vox:connection] handleMessage: tag=${message.payload.tag} lane=${message.lane_id}`);
    switch (message.payload.tag) {
      case "Ping":
        // r[impl connection.keepalive]
        void this.sendMessage({
          lane_id: 0n,
          payload: { tag: "Pong", value: { nonce: message.payload.value.nonce } },
        }).catch(() => {});
        return;

      case "Pong":
        // r[impl connection.keepalive]
        if (this.keepalivePendingNonce === message.payload.value.nonce) {
          clearTimeout(this.keepalivePongTimer!);
          this.keepalivePongTimer = null;
          this.keepalivePendingNonce = null;
          this.scheduleKeepalive();
        }
        return;

      case "ProtocolError":
        throw ConnectionError.peerProtocol(message.payload.value.description);

      case "LaneOpen":
        // r[impl lane.wire.compat]
        await this.handleLaneOpen(message.lane_id, message.payload.value);
        return;

      case "LaneAccept":
        // r[impl lane.wire.compat]
        this.handleLaneAccept(
          message.lane_id,
          message.payload.value.connection_settings,
          coerceMetadata(message.payload.value.metadata),
        );
        return;

      case "LaneReject":
        // r[impl lane.wire.compat]
        this.handleLaneReject(message.lane_id, message.payload.value);
        return;

      case "LaneClose":
        // r[impl lane.wire.compat]
        this.handleLaneClose(message.lane_id);
        return;

      case "RequestMessage":
        await this.handleRequestMessage(message.lane_id, message.payload.value);
        return;

      case "SchemaMessage":
        this.handleSchemaMessage(message.lane_id, message.payload.value);
        return;

      case "ChannelMessage":
        this.handleChannelMessage(message.lane_id, message.payload.value);
        return;

    }
  }

  private handleSchemaMessage(
    laneId: bigint,
    schemaMessage: SchemaMessage,
  ): void {
    const lane = this.getLane(laneId);
    const direction = schemaMessage.direction.tag === "Args" ? "args" : "response";
    try {
      // r[impl schema.tracking.received]
      lane.getSchemaTracker().recordReceived(
        schemaMessage.method_id,
        direction,
        new Uint8Array(schemaMessage.schemas),
      );
    } catch (error) {
      throw ConnectionError.protocol(error instanceof Error ? error.message : String(error));
    }
  }

  // r[impl connection.protocol-error]
  private async sendProtocolError(description: string): Promise<void> {
    try {
      await this.conduit.send(messageProtocolError(description));
    } catch (error) {
      voxLogger()?.debug("[vox:connection] failed to send protocol error", error);
    }
  }

  private async handleLaneOpen(
    laneId: bigint,
    value: { connection_settings: ConnectionSettings; metadata: unknown },
  ): Promise<void> {
    // r[impl lane.id.compat]
    // r[impl lane.service.compat]
    // r[impl lane.open.wire]
    // r[impl lane.open]
    // r[impl lane.accept.api]
    // r[impl lane.open.wire.rejection]
    // r[impl lane.open.settings]
    // r[impl lane.authorization]
    // r[impl lane.authorization.context]
    const establishmentContext: EstablishmentContext = {
      role: roleName(roleFromParity(this.localInitialLaneSettings.parity)),
      phase: "service-lane-open",
      laneId,
    };
    const establishmentStartedAt = observeEstablishmentStarted(
      this.observer,
      establishmentContext,
    );

    if (value.connection_settings.initial_channel_credit <= 0) {
      observeEstablishmentFinished(
        this.observer,
        establishmentContext,
        establishmentStartedAt,
        "error",
        "initial_channel_credit must be greater than zero",
      );
      await this.sendLaneReject(
        laneId,
        LaneRejection.withMessage(
          "policy-rejected",
          "initial_channel_credit must be greater than zero",
        ),
      );
      return;
    }

    const authorizationContext: EstablishmentContext = {
      role: establishmentContext.role,
      phase: "lane-authorization",
      laneId,
    };
    const authorizationStartedAt = observeEstablishmentStarted(
      this.observer,
      authorizationContext,
    );

    if (!this.onLane) {
      observeEstablishmentFinished(
        this.observer,
        authorizationContext,
        authorizationStartedAt,
        "rejected",
      );
      observeEstablishmentFinished(
        this.observer,
        establishmentContext,
        establishmentStartedAt,
        "rejected",
      );
      await this.sendLaneReject(
        laneId,
        LaneRejection.withMessage("not-ready", "no lane acceptor configured"),
      );
      return;
    }

    const localSettings: ConnectionSettings = {
      // r[impl lane.request-channel-parity]
      parity: oppositeParity(value.connection_settings.parity),
      max_concurrent_requests: value.connection_settings.max_concurrent_requests,
      initial_channel_credit: value.connection_settings.initial_channel_credit,
    };

    const metadata = coerceMetadata(value.metadata);
    const service = metadataString(metadata, "vox-service");
    if (!service) {
      observeEstablishmentFinished(
        this.observer,
        authorizationContext,
        authorizationStartedAt,
        "rejected",
      );
      await this.sendLaneReject(
        laneId,
        LaneRejection.withMessage("unknown-service", "missing required vox-service metadata"),
      );
      observeEstablishmentFinished(
        this.observer,
        establishmentContext,
        establishmentStartedAt,
        "rejected",
      );
      return;
    }

    const request: LaneRequest = {
      metadata,
      service,
      peerIdentity: this.peerIdentity,
      peerEvidence: this.peerEvidence,
    };
    const pending = new PendingLane(
      laneId,
      async (grant) => {
        const lane = new Lane(
          this,
          laneId,
          localSettings,
          value.connection_settings,
          grant,
        );
        this.lanes.set(laneId, lane);
        observeEstablishmentFinished(
          this.observer,
          authorizationContext,
          authorizationStartedAt,
          "ok",
        );
        const grantContext: EstablishmentContext = {
          role: establishmentContext.role,
          phase: "lane-grant",
          laneId,
        };
        const grantStartedAt = observeEstablishmentStarted(
          this.observer,
          grantContext,
        );
        try {
          await this.sendMessage(messageLaneAccept(laneId, localSettings, grant.metadata));
          observeEstablishmentFinished(
            this.observer,
            grantContext,
            grantStartedAt,
            "ok",
          );
        } catch (error) {
          this.lanes.delete(laneId);
          lane.close(error instanceof Error ? error : new ConnectionError(String(error)));
          observeEstablishmentFinished(
            this.observer,
            grantContext,
            grantStartedAt,
            "error",
            error,
          );
          observeEstablishmentFinished(
            this.observer,
            establishmentContext,
            establishmentStartedAt,
            "error",
            error,
          );
          throw error;
        }
        observeEstablishmentFinished(
          this.observer,
          establishmentContext,
          establishmentStartedAt,
          "ok",
        );
        return lane;
      },
      async (rejection) => {
        observeEstablishmentFinished(
          this.observer,
          authorizationContext,
          authorizationStartedAt,
          "rejected",
        );
        try {
          await this.sendLaneReject(laneId, rejection);
          observeEstablishmentFinished(
            this.observer,
            establishmentContext,
            establishmentStartedAt,
            "rejected",
          );
        } catch (error) {
          observeEstablishmentFinished(
            this.observer,
            establishmentContext,
            establishmentStartedAt,
            "error",
            error,
          );
          throw error;
        }
      },
    );

    try {
      await this.onLane(request, pending);
    } catch (error) {
      if (!pending.isHandled()) {
        await pending.reject(
          LaneRejection.withMessage(
            "policy-rejected",
            error instanceof Error ? error.message : String(error),
          ),
        );
        return;
      }
      throw error;
    }

    if (!pending.isHandled()) {
      await pending.reject(
        LaneRejection.withMessage(
          "policy-rejected",
          "lane acceptor returned without accepting or rejecting",
        ),
      );
    }
  }

  // r[impl lane.open.result]
  private async sendLaneReject(
    laneId: bigint,
    rejection: LaneRejection,
  ): Promise<void> {
    await this.sendMessage(messageLaneReject(laneId, rejection.toMetadata()));
  }

  private handleLaneAccept(
    laneId: bigint,
    peerSettings: ConnectionSettings,
    grantMetadata: Metadata,
  ): void {
    const pending = this.pendingLanes.get(laneId);
    if (!pending) {
      return;
    }
    this.pendingLanes.delete(laneId);
    const lane = new Lane(
      this,
      laneId,
      pending.localSettings,
      peerSettings,
      { metadata: grantMetadata },
    );
    this.lanes.set(laneId, lane);
    this.observeLaneGrantCreation(laneId, lane.laneGrant);
    observeEstablishmentFinished(
      this.observer,
      pending.establishmentContext,
      pending.establishmentStartedAt,
      "ok",
    );
    pending.result.resolve(lane);
  }

  // r[impl lane.open.result]
  private handleLaneReject(laneId: bigint, reject: LaneReject): void {
    const pending = this.pendingLanes.get(laneId);
    if (!pending) {
      return;
    }
    this.pendingLanes.delete(laneId);
    const rejection = LaneRejection.fromMetadata(coerceMetadata(reject.metadata));
    observeEstablishmentFinished(
      this.observer,
      pending.establishmentContext,
      pending.establishmentStartedAt,
      "rejected",
      rejection.message(),
    );
    pending.result.reject(ConnectionError.rejected(rejection));
  }

  private handleLaneClose(laneId: bigint): void {
    const lane = this.lanes.get(laneId);
    if (!lane) {
      return;
    }
    this.observeLaneGrantRevocation(laneId, lane);
    lane.close(ConnectionError.closed());
    this.lanes.delete(laneId);
  }

  private observeLaneGrantRevocation(laneId: bigint, lane: Lane): void {
    if (lane.laneGrant.metadata.size === 0) {
      return;
    }
    const context: EstablishmentContext = {
      role: this.establishmentRole(),
      phase: "lane-grant-revocation",
      laneId,
    };
    const startedAt = observeEstablishmentStarted(this.observer, context);
    observeEstablishmentFinished(this.observer, context, startedAt, "ok");
  }

  private observeLaneGrantCreation(laneId: bigint, grant: LaneGrant): void {
    if (grant.metadata.size === 0) {
      return;
    }
    const context: EstablishmentContext = {
      role: this.establishmentRole(),
      phase: "lane-grant",
      laneId,
    };
    const startedAt = observeEstablishmentStarted(this.observer, context);
    observeEstablishmentFinished(this.observer, context, startedAt, "ok");
  }

  private async handleRequestMessage(
    laneId: bigint,
    request: RequestMessage,
  ): Promise<void> {
    // r[impl rpc]
    // r[impl rpc.request]
    // r[impl rpc.request.id-allocation]
    // r[impl rpc.response]
    // r[impl rpc.cancel]
    // r[impl rpc.cancel.channels]
    // r[impl rpc.pipelining]
    // r[impl request.authorization]
    const lane = this.getLane(laneId);
    switch (request.body.tag) {
      case "Call": {
        const callSchemas = request.body.value.schemas;
        if (callSchemas && callSchemas.length > 0) {
          try {
            // r[impl schema.tracking.received]
            lane.getSchemaTracker().recordReceived(
              request.body.value.method_id,
              "args",
              new Uint8Array(callSchemas),
            );
          } catch (error) {
            throw ConnectionError.protocol(error instanceof Error ? error.message : String(error));
          }
        }
        try {
          // r[impl schema.exchange.required]
          lane.getSchemaTracker().requireReceived(request.body.value.method_id, "args");
        } catch (error) {
          throw ConnectionError.protocol(error instanceof Error ? error.message : String(error));
        }
        lane.beginIncomingRequest(request.id, request.body.value.channels);
        lane.enqueueIncomingCall({
          requestId: request.id,
          methodId: request.body.value.method_id,
          args: request.body.value.args,
          channels: request.body.value.channels,
          metadata: coerceMetadata(request.body.value.metadata),
          laneEpoch: lane.currentEpoch(),
          authorization: lane.requestAuthorization(this.peerIdentity, this.peerEvidence),
        });
        return;
      }

      case "Response": {
        const methodId = lane.pendingResponseMethodId(request.id);
        const responseSchemas = request.body.value.schemas;
        if (methodId !== undefined && responseSchemas && responseSchemas.length > 0) {
          try {
            // r[impl schema.tracking.received]
            lane.getSchemaTracker().recordReceived(
              methodId,
              "response",
              new Uint8Array(responseSchemas),
            );
          } catch (error) {
            throw ConnectionError.protocol(error instanceof Error ? error.message : String(error));
          }
        }
        if (methodId !== undefined) {
          try {
            // r[impl schema.exchange.required]
            lane.getSchemaTracker().requireReceived(methodId, "response");
          } catch (error) {
            throw ConnectionError.protocol(error instanceof Error ? error.message : String(error));
          }
        }
        lane.resolveResponse(request.id, request.body.value.ret);
        return;
      }

      case "Cancel":
        lane.enqueueIncomingCancel(request.id);
        return;
    }
  }

  private handleChannelMessage(
    laneId: bigint,
    channel: ChannelMessage,
  ): void {
    // r[impl rpc.channel.item]
    // r[impl rpc.channel.close]
    // r[impl rpc.channel.connection-closure]
    // r[impl rpc.channel.reset]
    // r[impl rpc.flow-control]
    // r[impl rpc.flow-control.credit.grant]
    const lane = this.getLane(laneId);
    switch (channel.body.tag) {
      case "Item":
        lane.routeChannelData(channel.id, channel.body.value.item);
        return;

      case "Close":
        lane.closeChannel(channel.id);
        return;

      case "Reset":
        lane.resetChannel(channel.id);
        return;

      case "GrantCredit":
        lane.grantChannelCredit(channel.id, channel.body.value.additional);
        return;
    }
  }

}

export class ConnectionHandle {
  private readonly core: ConnectionCore;

  constructor(core: ConnectionCore) {
    this.core = core;
  }

  peerIdentity(): PeerIdentity {
    // r[impl connection.identity]
    // r[impl connection.identity.scope]
    return this.core.peerIdentityValue();
  }

  peerEvidence(): PeerEvidence {
    // r[impl connection.evidence]
    return this.core.peerEvidenceValue();
  }

  openLane(
    settings: ConnectionSettings = this.core.defaultLaneSettings(),
    metadata: Metadata = emptyMetadata(),
  ): Promise<Lane> {
    return this.core.openLane(settings, metadata);
  }

  closeLane(laneId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    return this.core.closeLane(laneId, metadata);
  }

  shutdown(): void {
    // r[impl connection.shutdown.explicit]
    this.core.shutdown();
  }

  closed(): Promise<void> {
    return this.core.closedPromise();
  }
}

// r[impl lane]
export class Lane {
  private readonly role: Role;
  private readonly channelAllocator: ChannelIdAllocator;
  private readonly channelRegistry: ChannelRegistry;
  private readonly incomingCalls = new AsyncQueue<IncomingCall>();
  private readonly incomingCancels = new AsyncQueue<bigint>();
  private readonly pendingResponses = new Map<bigint, PendingResponse>();
  private readonly inboundLiveRequests = new Set<bigint>();
  private readonly inboundRequestChannels = new Map<bigint, bigint[]>();
  private readonly outboundRequestLimit: AsyncSemaphore;
  private readonly schemaTracker = new SchemaTracker();
  private readonly schemaSendTracker = new SchemaSendTracker();
  private epoch = 0;
  private nextRequestId: bigint;
  private closed = false;
  private flushPromise: Promise<void> | null = null;
  private flushRequested = false;
  private callerRefs = 0;

  private readonly connection: ConnectionCore;
  readonly id: bigint;
  readonly localSettings: ConnectionSettings;
  readonly peerSettings: ConnectionSettings;
  readonly laneGrant: LaneGrant;

  constructor(
    connection: ConnectionCore,
    id: bigint,
    localSettings: ConnectionSettings,
    peerSettings: ConnectionSettings,
    laneGrant: LaneGrant = emptyLaneGrant(),
  ) {
    this.connection = connection;
    this.id = id;
    this.localSettings = localSettings;
    this.peerSettings = peerSettings;
    this.laneGrant = laneGrant;
    // r[impl rpc.flow-control.max-concurrent-requests]
    // r[impl rpc.flow-control.max-concurrent-requests.outbound]
    this.outboundRequestLimit = new AsyncSemaphore(peerSettings.max_concurrent_requests);
    // r[impl lane.request-channel-parity]
    this.role = roleFromParity(localSettings.parity);
    this.channelAllocator = new ChannelIdAllocator(this.role);
    this.channelRegistry = new ChannelRegistry(undefined, () => {
      void this.flushOutgoing();
    });
    // r[impl lane.request-channel-parity]
    this.nextRequestId = firstIdForParity(localSettings.parity);
  }

  connectionHandle(): ConnectionHandle {
    return this.connection.connectionHandleValue();
  }

  caller(): Caller {
    return new LaneCaller(this, this.retainCaller());
  }

  getChannelAllocator(): ChannelIdAllocator {
    return this.channelAllocator;
  }

  getChannelRegistry(): ChannelRegistry {
    return this.channelRegistry;
  }

  getSchemaTracker(): SchemaTracker {
    return this.schemaTracker;
  }

  getSchemaSendTracker(): import("./schema_tracker.ts").SchemaSendTracker {
    return this.schemaSendTracker;
  }

  currentEpoch(): number {
    return this.epoch;
  }

  requestAuthorization(
    peerIdentity: PeerIdentity,
    peerEvidence: PeerEvidence,
  ): RequestAuthorizationContext {
    // r[impl request.authorization]
    return requestAuthorizationContext(peerIdentity, peerEvidence, this.laneGrant);
  }

  // r[impl rpc.debug.snapshot]
  // r[impl rpc.observability.channel.context]
  debugSnapshot(): LaneDebugSnapshot {
    return {
      laneId: this.id,
      closed: this.closed,
      pendingResponseCount: this.pendingResponses.size,
      pendingRequestIds: [...this.pendingResponses.keys()],
      inboundLiveRequestCount: this.inboundLiveRequests.size,
      inboundLiveRequestIds: [...this.inboundLiveRequests],
      flowControl: {
        localMaxConcurrentRequests: this.localSettings.max_concurrent_requests,
        peerMaxConcurrentRequests: this.peerSettings.max_concurrent_requests,
        outboundRequestLimit: this.outboundRequestLimit.debugSnapshot(),
      },
      channels: this.channelRegistry.debugSnapshot(),
    };
  }

  isClosed(): boolean {
    return this.closed;
  }

  async call(request: CallerRequest): Promise<unknown> {
    if (this.closed) {
      throw ConnectionError.closed();
    }

    const { methodSchemas, registry } = request;
    const channelMetas = methodSchemas.channels;
    const hasChannels = channelMetas.length > 0;
    const channelCredit = {
      outgoing: this.peerSettings.initial_channel_credit ?? DEFAULT_INITIAL_CREDIT,
      incoming: this.localSettings.initial_channel_credit ?? DEFAULT_INITIAL_CREDIT,
    };
    // Bind any `Tx`/`Rx` arguments (out-of-band index design): allocate channel
    // ids, bind the local-facing handles with a phon per-item codec, and replace
    // each channel arg with its wire-index `Bytes` before encoding the tuple.
    const encodeWithChannels = (): { payload: Uint8Array; channels: bigint[] } => {
      const rawValues = Object.values(request.args);
      if (!hasChannels) {
        const payload = rawValues.length === 0
          ? new Uint8Array(0)
          : encodeTyped(rawValues as never, methodSchemas.argsRoot, registry);
        return { payload, channels: request.channels ?? [] };
      }
      const bound = bindPhonChannels(
        rawValues,
        channelMetas,
        this.channelAllocator,
        this.channelRegistry,
        registry,
        channelCredit,
        {
          methodId: request.descriptor.id,
          direction: "args",
          tracker: this.getSchemaTracker(),
        },
      );
      finalizeChannels = bound.finalize;
      const payload = encodeTyped(bound.values as never, methodSchemas.argsRoot, registry);
      return { payload, channels: bound.channels };
    };

    let finalizeChannels = request.finalizeChannels;
    const initial = encodeWithChannels();
    // r[impl rpc.flow-control.max-concurrent-requests.outbound]
    // r[impl rpc.flow-control.max-concurrent-requests.counting]
    let requestPermit;
    try {
      requestPermit = await this.outboundRequestLimit.acquire();
    } catch (error) {
      finalizeChannels?.();
      throw error;
    }

    const metadataCarrier = request.metadata ? request.metadata.clone() : new ClientMetadata();
    const metadata = metadataCarrier.toWire();
    const requestId = this.nextRequestId;
    this.nextRequestId += 2n;
    this.rememberCallerChannelContexts(requestId, request, initial.channels);
    let responsePayload: Uint8Array;
    try {
      // r[impl rpc.request.scope]
      responsePayload = await new Promise<Uint8Array>((resolve, reject) => {
        // The args schema closure is advertised once per (method, connection).
        const computeSchemas: () => number[] = () =>
          this.getSchemaSendTracker().prepareSchemas(
            request.descriptor.id,
            "args",
            methodSchemas.argsSchemaClosure,
          );
        const idleTimeoutMs = request.timeoutMs ?? DEFAULT_TIMEOUT_MS;
        const startedAt = Date.now();

        let state!: PendingResponse;
        state = {
          resolve,
          reject,
          timer: setTimeout(() => {
            this.timeoutPendingResponse(state);
          }, idleTimeoutMs),
          idleTimeoutMs,
          lastProgressAt: startedAt,
          methodId: request.descriptor.id,
          payload: initial.payload.slice(),
          metadata: cloneMetadata(metadata),
          channels: [...initial.channels],
          computeSchemas,
          finalizeChannels,
          requestIds: new Set(),
          settled: false,
        };

        this.pendingResponses.set(requestId, state);
        state.requestIds.add(requestId);

        void this.sendPendingRequest(state, requestId, true);
      });
    } finally {
      requestPermit.release();
    }

    // Decode the response against the server's advertised `Result<T, VoxError<E>>`
    // schema binding. The connection receive path enforces that the binding arrived
    // before resolving this payload; reaching this point without it is a local invariant
    // failure, not a same-schema shortcut.
    // r[impl schema.errors.call-level]
    // r[impl schema.errors.call-level.caller]
    // r[impl schema.errors.same-peer-terminal]
    this.getSchemaTracker().requireReceived(request.descriptor.id, "response");
    const decoder = this.getSchemaTracker().buildWriterDecoder(
      request.descriptor.id,
      "response",
      registry,
    );
    if (!decoder) {
      throw new SchemaCompatibilityError(
        `missing response schema binding for method ${request.descriptor.id}`,
      );
    }
    const decoded = decoder(responsePayload) as unknown as {
      tag?: string;
      ok?: boolean;
      value?: unknown;
      error?: unknown;
    };

    if (decoded.tag === "Ok" || decoded.ok === true) {
      return decoded.value;
    }

    const err = (decoded.tag === "Err" ? decoded.value : decoded.error) as { tag: string; value?: unknown };
    switch (err.tag) {
      case "User":
        throw new RpcError(RpcErrorCode.USER, null, err.value);
      case "UnknownMethod":
        throw new RpcError(RpcErrorCode.UNKNOWN_METHOD);
      case "InvalidPayload":
        throw new RpcError(RpcErrorCode.INVALID_PAYLOAD);
      case "Cancelled":
        throw new RpcError(RpcErrorCode.CANCELLED);
      case "TimedOut":
        throw new RpcError(RpcErrorCode.TIMED_OUT);
      case "ConnectionClosed":
      case "ConnectionShutdown":
      case "SendFailed":
      case "Indeterminate":
        throw new RpcError(RpcErrorCode.INDETERMINATE);
      default:
        throw new RpcError(RpcErrorCode.INVALID_PAYLOAD);
    }
  }

  async sendResponse(
    requestId: bigint,
    payload: Uint8Array,
    metadata: Metadata = emptyMetadata(),
    _channels: bigint[] = [],
    schemas: number[] = [],
  ): Promise<void> {
    try {
      await this.connection.sendMessage(messageResponse(requestId, payload, metadata, this.id, schemas));
    } finally {
      // r[impl rpc.request.scope.terminal]
      // r[impl rpc.request.scope.channels]
      this.finishIncomingRequest(requestId);
    }
  }

  async sendCancel(requestId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    await this.connection.sendMessage(messageCancel(requestId, this.id, metadata));
  }

  async sendChannelData(channelId: bigint, payload: Uint8Array): Promise<void> {
    await this.connection.sendMessage(messageData(channelId, payload, this.id));
    this.markChannelRequestProgress(channelId);
  }

  async sendChannelClose(channelId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    await this.connection.sendMessage(messageClose(channelId, this.id, metadata));
    this.markChannelRequestProgress(channelId);
  }

  async sendChannelCredit(channelId: bigint, additional: number): Promise<void> {
    await this.connection.sendMessage(messageCredit(channelId, additional, this.id));
    this.markChannelRequestProgress(channelId);
  }

  async sendSchemas(
    methodId: bigint,
    direction: "args" | "response",
    schemas: Uint8Array,
  ): Promise<void> {
    await this.connection.sendMessage(messageSchema(methodId, direction, Array.from(schemas), this.id));
  }

  enqueueIncomingCall(call: IncomingCall): void {
    this.incomingCalls.push(call);
  }

  beginIncomingRequest(requestId: bigint, channels: bigint[] = []): void {
    // r[impl rpc.request.scope]
    // r[impl rpc.flow-control.max-concurrent-requests]
    // r[impl rpc.flow-control.max-concurrent-requests.inbound]
    if (this.inboundLiveRequests.has(requestId)) {
      throw ConnectionError.protocol(`duplicate live request ${requestId}`);
    }
    if (this.inboundLiveRequests.size >= this.localSettings.max_concurrent_requests) {
      throw ConnectionError.protocol(
        `max_concurrent_requests exceeded for connection ${this.id}`,
      );
    }
    this.inboundLiveRequests.add(requestId);
    this.inboundRequestChannels.set(requestId, [...channels]);
  }

  finishIncomingRequest(requestId: bigint, error = ChannelError.requestClosed()): void {
    // r[impl rpc.request.scope.terminal]
    // r[impl rpc.request.scope.channels]
    // r[impl rpc.flow-control.max-concurrent-requests.counting]
    this.inboundLiveRequests.delete(requestId);
    const channels = this.inboundRequestChannels.get(requestId) ?? [];
    this.inboundRequestChannels.delete(requestId);
    for (const channelId of channels) {
      this.channelRegistry.reset(channelId, error);
    }
  }

  nextIncomingCall(): Promise<IncomingCall | null> {
    return this.incomingCalls.shift();
  }

  enqueueIncomingCancel(requestId: bigint): void {
    this.markRequestProgress(requestId);
    this.finishIncomingRequest(requestId, ChannelError.cancelled());
    this.incomingCancels.push(requestId);
  }

  nextIncomingCancel(): Promise<bigint | null> {
    return this.incomingCancels.shift();
  }

  resolveResponse(requestId: bigint, payload: Uint8Array): void {
    const state = this.pendingResponses.get(requestId);
    if (!state || state.settled) {
      return;
    }
    voxLogger()?.debug(
      `[vox:connection] resolveResponse: req=${requestId} payload=${payload.length}`,
    );
    this.markRequestProgress(requestId);
    // r[impl rpc.request.scope.terminal]
    this.clearPendingState(state);
    state.resolve(payload);
  }

  pendingResponseMethodId(requestId: bigint): bigint | undefined {
    return this.pendingResponses.get(requestId)?.methodId;
  }

  routeChannelData(channelId: bigint, payload: Uint8Array): void {
    try {
      this.channelRegistry.routeData(channelId, payload);
      this.markChannelRequestProgress(channelId);
    } catch (error) {
      if (error instanceof ChannelError) {
        this.close(new ConnectionError(error.message));
        return;
      }
      throw error;
    }
  }

  closeChannel(channelId: bigint): void {
    this.channelRegistry.close(channelId);
    this.markChannelRequestProgress(channelId);
  }

  resetChannel(channelId: bigint): void {
    this.channelRegistry.reset(channelId);
    this.markChannelRequestProgress(channelId);
  }

  grantChannelCredit(channelId: bigint, additional: number): void {
    this.channelRegistry.grantCredit(channelId, additional);
    this.markChannelRequestProgress(channelId);
  }

  close(error: Error): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    this.incomingCalls.close();
    this.incomingCancels.close();
    this.channelRegistry.closeAll(ChannelError.connectionClosed());
    this.inboundLiveRequests.clear();
    this.inboundRequestChannels.clear();
    // r[impl rpc.flow-control.max-concurrent-requests.connection-failure]
    this.outboundRequestLimit.close(error);
    const pendingStates = new Set(this.pendingResponses.values());
    this.pendingResponses.clear();
    for (const pending of pendingStates) {
      if (pending.settled) {
        continue;
      }
      pending.settled = true;
      clearTimeout(pending.timer);
      pending.requestIds.clear();
      pending.finalizeChannels?.();
      pending.reject(error);
    }
  }

  failPendingAttempts(error: Error): void {
    this.channelRegistry.closeAll(ChannelError.connectionClosed());
    this.outboundRequestLimit.close(error);
    const pendingStates = new Set(this.pendingResponses.values());
    this.pendingResponses.clear();
    for (const pending of pendingStates) {
      if (pending.settled) {
        continue;
      }
      pending.settled = true;
      clearTimeout(pending.timer);
      pending.requestIds.clear();
      pending.finalizeChannels?.();
      pending.reject(error);
    }
  }

  private retainCaller(): () => void {
    // r[impl rpc.caller.liveness.refcounted]
    // r[impl rpc.caller.liveness.last-drop-closes-connection]
    this.callerRefs += 1;
    let released = false;
    return () => {
      if (released) {
        return;
      }
      released = true;
      this.callerRefs -= 1;
    };
  }

  private clearPendingState(
    state: PendingResponse,
    options: { finalizeChannels?: boolean; channelError?: ChannelError } = {},
  ): void {
    // r[impl rpc.request.scope.terminal]
    // r[impl rpc.request.scope.channels]
    if (state.settled) {
      return;
    }
    state.settled = true;
    clearTimeout(state.timer);
    for (const requestId of state.requestIds) {
      this.pendingResponses.delete(requestId);
    }
    state.requestIds.clear();
    if (options.finalizeChannels !== false) {
      const channelError = options.channelError ?? ChannelError.requestClosed();
      for (const channelId of state.channels) {
        this.channelRegistry.reset(channelId, channelError);
      }
      state.finalizeChannels?.();
    }
  }

  // r[impl rpc.timeout.idle-progress]
  private markRequestProgress(requestId: bigint): void {
    const state = this.pendingResponses.get(requestId);
    if (!state) {
      return;
    }
    this.resetPendingResponseIdle(state);
  }

  // r[impl rpc.timeout.idle-progress]
  private markChannelRequestProgress(channelId: bigint): void {
    const seen = new Set<PendingResponse>();
    for (const state of this.pendingResponses.values()) {
      if (seen.has(state) || !state.channels.includes(channelId)) {
        continue;
      }
      seen.add(state);
      this.resetPendingResponseIdle(state);
    }
  }

  private resetPendingResponseIdle(state: PendingResponse): void {
    if (state.settled) {
      return;
    }
    state.lastProgressAt = Date.now();
    clearTimeout(state.timer);
    state.timer = setTimeout(() => {
      this.timeoutPendingResponse(state);
    }, state.idleTimeoutMs);
  }

  // r[impl rpc.timeout.idle-progress]
  private timeoutPendingResponse(state: PendingResponse): void {
    if (state.settled) {
      return;
    }

    const remainingMs = state.idleTimeoutMs - (Date.now() - state.lastProgressAt);
    if (remainingMs > 0) {
      state.timer = setTimeout(() => {
        this.timeoutPendingResponse(state);
      }, remainingMs);
      return;
    }

    const requestIds = [...state.requestIds];
    this.clearPendingState(state, { channelError: ChannelError.timedOut() });
    for (const requestId of requestIds) {
      void this.sendCancel(requestId).catch((error) => {
        voxLogger()?.debug("[vox:connection] failed to send cancel after request idle timeout", error);
      });
    }
    state.reject(new RpcError(RpcErrorCode.TIMED_OUT));
  }

  private async sendPendingRequest(
    state: PendingResponse,
    requestId = this.allocateRequestId(),
    rejectOnFailure = false,
  ): Promise<void> {
    if (state.settled || this.closed) {
      return;
    }
    this.pendingResponses.set(requestId, state);
    state.requestIds.add(requestId);

    try {
      // r[impl schema.exchange.caller]
      const schemas = state.computeSchemas?.();
      voxLogger()?.debug(
        `[vox:connection] sendPendingRequest: req=${requestId} method=${state.methodId} payload=${state.payload.length} channels=${state.channels.length} schemas=${schemas?.length ?? 0}`,
      );
      await this.connection.sendMessage(
        messageRequest(
          requestId,
          state.methodId,
          state.payload,
          cloneMetadata(state.metadata),
          [...state.channels],
          this.id,
          schemas,
        ),
      );
      this.markRequestProgress(requestId);
      await this.flushOutgoing();
    } catch (error) {
      state.requestIds.delete(requestId);
      this.pendingResponses.delete(requestId);
      if (!rejectOnFailure || state.settled) {
        return;
      }
      this.clearPendingState(state);
      state.reject(error instanceof Error ? error : new ConnectionError(String(error)));
    }
  }

  private allocateRequestId(): bigint {
    // r[impl lane.request-channel-parity]
    const requestId = this.nextRequestId;
    this.nextRequestId += 2n;
    return requestId;
  }

  private rememberCallerChannelContexts(
    requestId: bigint,
    request: CallerRequest,
    channels: bigint[],
  ): void {
    if (channels.length === 0) {
      return;
    }
    const { service, method } = splitQualifiedMethodName(request.method);
    const metas = [...request.methodSchemas.channels].sort((a, b) => a.index - b.index);
    for (let index = 0; index < channels.length; index += 1) {
      const meta = metas[index];
      this.channelRegistry.rememberContext(channels[index]!, {
        laneId: this.id,
        requestId,
        service,
        method,
        channelDirection: meta?.direction === "tx" ? "rx" : "tx",
        side: "client",
      });
    }
  }

  async flushOutgoing(): Promise<void> {
    if (this.closed) {
      return;
    }
    this.flushRequested = true;
    if (this.flushPromise) {
      await this.flushPromise;
      return;
    }

    const flush = (async () => {
      while (!this.closed) {
        this.flushRequested = false;
        while (!this.closed) {
          const poll = this.channelRegistry.pollOutgoing();
          if (poll.kind === "pending" || poll.kind === "done") {
            break;
          }
          switch (poll.kind) {
            case "data":
              await this.sendChannelData(poll.channelId, poll.payload);
              break;
            case "close":
              await this.sendChannelClose(poll.channelId);
              break;
            case "credit":
              await this.sendChannelCredit(poll.channelId, poll.additional);
              break;
          }
        }
        if (!this.flushRequested) {
          return;
        }
      }
    })();

    this.flushPromise = flush;
    try {
      await flush;
    } finally {
      if (this.flushPromise === flush) {
        this.flushPromise = null;
      }
    }
  }
}

class LaneCaller implements Caller {
  private readonly lane: Lane;
  private readonly releaseCaller: () => void;
  private disposed = false;

  constructor(lane: Lane, releaseCaller: () => void) {
    this.lane = lane;
    this.releaseCaller = releaseCaller;
  }

  call(request: CallerRequest): Promise<unknown> {
    return this.lane.call(request);
  }

  getChannelAllocator(): ChannelIdAllocator {
    return this.lane.getChannelAllocator();
  }

  getChannelRegistry(): ChannelRegistry {
    return this.lane.getChannelRegistry();
  }

  with(middleware: ClientMiddleware): Caller {
    return new MiddlewareCaller(this, [middleware]);
  }

  dispose(): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    this.releaseCaller();
  }
}

function metadataForLaneClient<T>(
  clientCtor: LaneClientConstructor<T>,
  metadata: Metadata | undefined,
): Metadata {
  if (metadata) {
    return metadata;
  }
  return clientCtor.descriptor
    ? voxServiceMetadata(clientCtor.descriptor.service_name)
    : emptyMetadata();
}

function connectionOptionsForConnectLane(options: ConnectLaneOptions): ConnectionTransportOptions {
  const {
    laneSettings: _laneSettings,
    laneMetadata: _laneMetadata,
    ...connectionOptions
  } = options;
  return connectionOptions;
}

export function defaultLaneSettings(
  role: Role,
  maxConcurrentRequests = 64,
  channelCapacity = DEFAULT_CHANNEL_CAPACITY,
): ConnectionSettings {
  // r[impl rpc.flow-control.credit.initial.high-level]
  // r[impl rpc.flow-control.credit.initial.zero]
  if (channelCapacity <= 0) {
    throw ConnectionError.protocol("initial_channel_credit must be greater than zero");
  }

  return {
    // r[impl connection.lane-id-parity]
    parity: parityFromRole(role),
    // r[impl rpc.flow-control.max-concurrent-requests.default]
    max_concurrent_requests: maxConcurrentRequests,
    initial_channel_credit: channelCapacity,
  };
}

// r[impl rpc.caller.liveness.public-handle-drop]
export class Connection {
  private readonly core: ConnectionCore;

  private constructor(core: ConnectionCore) {
    this.core = core;
  }

  static connectConduit(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    options: ConnectionBuilderOptions = {},
  ): Connection {
    // r[impl connection.model]
    // r[impl connection.lifecycle.driven]
    const core = new ConnectionCore(
      conduit,
      handshake.localSettings,
      handshake.peerSettings,
      handshake.peerIdentity,
      handshake.peerEvidence,
      options.onLane,
      options.keepaliveIntervalMs ?? 0,
      options.keepaliveTimeoutMs ?? 0,
      options.observer,
    );
    core.initialLane();
    core.start();
    return new Connection(core);
  }

  static acceptConduit(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    options: ConnectionBuilderOptions = {},
  ): Connection {
    return Connection.connectConduit(conduit, handshake, options);
  }

  handle(): ConnectionHandle {
    return this.core.connectionHandleValue();
  }

  openRawLane(options: LaneOpenOptions = {}): Promise<Lane> {
    return this.core.openLane(
      options.settings ?? this.core.defaultLaneSettings(),
      options.metadata ?? emptyMetadata(),
    );
  }

  async openLane<T>(
    clientCtor: LaneClientConstructor<T>,
    options: LaneOpenOptions = {},
  ): Promise<T> {
    const lane = await this.openRawLane({
      settings: options.settings,
      metadata: metadataForLaneClient(clientCtor, options.metadata),
    });
    return new clientCtor(lane.caller());
  }

  closed(): Promise<void> {
    return this.core.closedPromise();
  }
}

export async function connect(
  transport: ConnectionTransport,
  options: ConnectionTransportOptions = {},
): Promise<Connection> {
  const established = await makeInitiatorEstablishedTransport(transport, options);
  return Connection.connectConduit(
    established.conduit,
    established.handshake,
    options,
  );
}

export async function accept(
  transport: ConnectionTransport,
  options: ConnectionTransportOptions = {},
): Promise<Connection> {
  const established = await makeAcceptorEstablishedTransport(transport, options);
  return Connection.acceptConduit(
    established.conduit,
    established.handshake,
    options,
  );
}

export async function connectOnLink(
  link: Link,
  options: ConnectionTransportOptions = {},
): Promise<Connection> {
  const localSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: channelCapacityFromOptions(options),
  };
  const handshake = await observeEstablishment(
    options.observer,
    { role: "initiator", phase: "connection-handshake" },
    () => handshakeAsInitiator(
      link,
      localSettings,
      options.metadata ?? emptyMetadata(),
      {
        identityResolver: options.identityResolver,
        observer: options.observer,
      },
    ),
  );
  const messagePlan = await observeEstablishment(
    options.observer,
    { role: "initiator", phase: "schema-decode-plan" },
    () => buildMessageDecodePlan(handshake.peerMessageSchema),
  );
  return Connection.connectConduit(
    new BareConduit(link, messagePlan),
    handshake,
    options,
  );
}

export async function acceptOnLink(
  link: Link,
  options: ConnectionTransportOptions = {},
): Promise<Connection> {
  const localSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: channelCapacityFromOptions(options),
  };
  const handshake = await observeEstablishment(
    options.observer,
    { role: "acceptor", phase: "connection-handshake" },
    () => handshakeAsAcceptor(
      link,
      localSettings,
      options.metadata ?? emptyMetadata(),
      {
        identityResolver: options.identityResolver,
        observer: options.observer,
      },
    ),
  );
  const messagePlan = await observeEstablishment(
    options.observer,
    { role: "acceptor", phase: "schema-decode-plan" },
    () => buildMessageDecodePlan(handshake.peerMessageSchema),
  );
  return Connection.acceptConduit(
    new BareConduit(link, messagePlan),
    handshake,
    options,
  );
}

export async function connectLane<T>(
  transport: ConnectionTransport,
  clientCtor: LaneClientConstructor<T>,
  options: ConnectLaneOptions = {},
): Promise<T> {
  const connection = await connect(transport, connectionOptionsForConnectLane(options));
  return connection.openLane(clientCtor, {
    settings: options.laneSettings,
    metadata: options.laneMetadata,
  });
}
