import { encodeTyped } from "@bearcove/phon-engine";
import {
  type ConnectionSettings,
  type RequestMessage,
  type ChannelMessage,
  type SchemaMessage,
  type Message,
  type Metadata,
  emptyMetadata,
  coerceMetadata,
  messageAccept,
  messageConnect,
  messageGoodbye,
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
  handshakeAsAcceptor,
  handshakeAsInitiator,
  type HandshakeResult,
} from "./handshake.ts";
import { splitQualifiedMethodName } from "./observer.ts";

const DEFAULT_TIMEOUT_MS = 30_000;

interface PendingResponse {
  resolve: (payload: Uint8Array) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
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
  requestId: bigint;
  methodId: bigint;
  args: Uint8Array;
  channels: bigint[];
  metadata: Metadata;
  connectionEpoch: number;
}

export interface ConnectionDebugSnapshot {
  connectionId: bigint;
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

export interface SessionBuilderOptions {
  maxConcurrentRequests?: number;
  channelCapacity?: number;
  metadata?: Metadata;
  onConnection?: (connection: ConnectionHandle) => void | Promise<void>;
  /**
   * If set, the session sends a Ping every `keepaliveIntervalMs` milliseconds
   * and expects a Pong back within `keepaliveTimeoutMs` (default: half the
   * interval). If no Pong arrives in time the session closes. Set to 0 or
   * undefined to disable keepalive.
   *
   * Recommended for WebSocket connections where silent network drops are
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

export interface SessionTransportOptions extends SessionBuilderOptions {
}

const DEFAULT_CHANNEL_CAPACITY = 16;

function channelCapacityFromOptions(options: SessionBuilderOptions): number {
  const channelCapacity = options.channelCapacity ?? DEFAULT_CHANNEL_CAPACITY;
  // r[impl rpc.flow-control.credit.initial.high-level]
  // r[impl rpc.flow-control.credit.initial.zero]
  if (channelCapacity <= 0) {
    throw SessionError.protocol("initial_channel_credit must be greater than zero");
  }
  return channelCapacity;
}

type SessionTransport = Link | LinkSource;

function isLinkSource(value: SessionTransport): value is LinkSource {
  return typeof (value as LinkSource).nextLink === "function";
}

function cloneMetadata(metadata: Metadata): Metadata {
  return new Map(metadata);
}

interface EstablishedTransport {
  conduit: Conduit<Message>;
  handshake: HandshakeResult;
}

// r[impl session]
// r[impl session.handshake]
// r[impl session.handshake.phon]
// r[impl session.handshake.protocol-schema]
// r[impl session.handshake.protocol-schema.session-scoped]
// r[impl session.handshake.unversioned]
// r[impl session.connection-settings]
// r[impl session.connection-settings.hello]
// r[impl session.peer]
// r[impl session.role]
// r[impl session.symmetry]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.post-accept]
async function makeInitiatorEstablishedTransport(
  transport: SessionTransport,
  options: SessionTransportOptions,
): Promise<EstablishedTransport> {
  const localSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    // r[impl rpc.flow-control.max-concurrent-requests.default]
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: channelCapacityFromOptions(options),
  };

  if (isLinkSource(transport)) {
    const attachment = await transport.nextLink();
    await performInitiatorTransportPrologue(attachment.link);
    const handshake = await handshakeAsInitiator(
      attachment.link,
      localSettings,
      options.metadata ?? emptyMetadata(),
    );
    const messagePlan = buildMessageDecodePlan(handshake.peerMessageSchema);

    return {
      conduit: new BareConduit(attachment.link, messagePlan),
      handshake,
    };
  }

  await performInitiatorTransportPrologue(transport);
  const handshake = await handshakeAsInitiator(
    transport,
    localSettings,
    options.metadata ?? emptyMetadata(),
  );
  const messagePlan = buildMessageDecodePlan(handshake.peerMessageSchema);

  return {
    conduit: new BareConduit(transport, messagePlan),
    handshake,
  };
}

// r[impl session]
// r[impl session.handshake]
// r[impl session.handshake.phon]
// r[impl session.handshake.protocol-schema]
// r[impl session.handshake.protocol-schema.session-scoped]
// r[impl session.handshake.sorry]
// r[impl session.handshake.unversioned]
// r[impl session.connection-settings]
// r[impl session.connection-settings.hello]
// r[impl session.peer]
// r[impl session.role]
// r[impl session.symmetry]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.post-accept]
async function makeAcceptorEstablishedTransport(
  transport: SessionTransport,
  options: SessionTransportOptions,
): Promise<EstablishedTransport> {
  const attachment = isLinkSource(transport)
    ? await transport.nextLink()
    : { link: transport };
  await performAcceptorTransportPrologue(attachment.link);

  const localSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    // r[impl rpc.flow-control.max-concurrent-requests.default]
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: channelCapacityFromOptions(options),
  };

  const handshake = await handshakeAsAcceptor(
    attachment.link,
    localSettings,
    options.metadata ?? emptyMetadata(),
  );
  const messagePlan = buildMessageDecodePlan(handshake.peerMessageSchema);

  return {
    conduit: new BareConduit(attachment.link, messagePlan),
    handshake,
  };
}

export class SessionError extends Error {
  readonly isProtocolError: boolean;
  readonly receivedFromPeer: boolean;

  constructor(
    message: string,
    options: { isProtocolError?: boolean; receivedFromPeer?: boolean } = {},
  ) {
    super(message);
    this.name = "SessionError";
    this.isProtocolError = options.isProtocolError ?? false;
    this.receivedFromPeer = options.receivedFromPeer ?? false;
  }

  static closed(): SessionError {
    return new SessionError("session closed");
  }

  static protocol(message: string): SessionError {
    return new SessionError(message, { isProtocolError: true });
  }

  static peerProtocol(message: string): SessionError {
    return new SessionError(message, { isProtocolError: true, receivedFromPeer: true });
  }
}

class SessionCore {
  private conduit: Conduit<Message>;
  private readonly connections = new Map<bigint, ConnectionHandle>();
  private readonly pendingConnections = new Map<
    bigint,
    {
      localSettings: ConnectionSettings;
      result: Deferred<ConnectionHandle>;
    }
  >();
  private readonly sessionHandle: SessionHandle;
  private sendChain: Promise<void> = Promise.resolve();
  private nextConnectionId: bigint;
  private closed = false;
  private closeError: SessionError | null = null;
  private rootConnectionValue: ConnectionHandle | null = null;
  private runPromise: Promise<void> | null = null;
  private readonly keepaliveIntervalMs: number;
  private readonly keepaliveTimeoutMs: number;
  private keepaliveTimer: ReturnType<typeof setTimeout> | null = null;
  private keepalivePendingNonce: bigint | null = null;
  private keepalivePongTimer: ReturnType<typeof setTimeout> | null = null;
  private nextKeepaliveNonce = 1n;
  private readonly localRootSettings: ConnectionSettings;
  private readonly peerRootSettings: ConnectionSettings;
  private readonly onConnection?: (connection: ConnectionHandle) => void | Promise<void>;
  private rootInternallyClosed = false;

  constructor(
    conduit: Conduit<Message>,
    localRootSettings: ConnectionSettings,
    peerRootSettings: ConnectionSettings,
    onConnection?: (connection: ConnectionHandle) => void | Promise<void>,
    keepaliveIntervalMs = 0,
    keepaliveTimeoutMs = 0,
  ) {
    this.conduit = conduit;
    this.localRootSettings = localRootSettings;
    this.peerRootSettings = peerRootSettings;
    // r[impl session.parity]
    this.nextConnectionId = firstIdForParity(localRootSettings.parity);
    this.sessionHandle = new SessionHandle(this);
    this.onConnection = onConnection;
    this.keepaliveIntervalMs = keepaliveIntervalMs;
    this.keepaliveTimeoutMs =
      keepaliveTimeoutMs > 0 ? keepaliveTimeoutMs : Math.floor(keepaliveIntervalMs / 2);
  }

  sessionHandleValue(): SessionHandle {
    return this.sessionHandle;
  }

  defaultConnectionSettings(): ConnectionSettings {
    return {
      parity: this.localRootSettings.parity,
      max_concurrent_requests: this.localRootSettings.max_concurrent_requests,
      initial_channel_credit: this.localRootSettings.initial_channel_credit,
    };
  }

  rootConnection(): ConnectionHandle {
    // r[impl connection]
    // r[impl connection.root]
    if (!this.rootConnectionValue) {
      this.rootConnectionValue = new ConnectionHandle(
        this,
        0n,
        this.localRootSettings,
        this.peerRootSettings,
      );
      this.connections.set(0n, this.rootConnectionValue);
    }
    return this.rootConnectionValue;
  }

  failPendingAttempts(error: Error): void {
    for (const connection of this.connections.values()) {
      connection.failPendingAttempts(error);
    }
  }

  start(): void {
    if (this.runPromise) {
      return;
    }
    this.runPromise = this.run().catch(async (error) => {
      voxLogger()?.error(`[vox:session] run loop error:`, error);
      const sessionError = error instanceof SessionError
        ? error
        : new SessionError(String(error));
      if (
        sessionError.isProtocolError &&
        !sessionError.receivedFromPeer &&
        !this.closed
      ) {
        await this.sendProtocolError(sessionError.message);
      }
      this.fail(sessionError);
    });
    // r[impl session.keepalive]
    if (this.keepaliveIntervalMs > 0) {
      this.scheduleKeepalive();
    }
  }

  // r[impl session.keepalive]
  private scheduleKeepalive(): void {
    if (this.closed || this.keepaliveIntervalMs <= 0) return;
    this.keepaliveTimer = setTimeout(() => {
      this.sendKeepalivePing();
    }, this.keepaliveIntervalMs);
  }

  // r[impl session.keepalive]
  private sendKeepalivePing(): void {
    if (this.closed) {
      this.scheduleKeepalive();
      return;
    }
    const nonce = this.nextKeepaliveNonce++;
    this.keepalivePendingNonce = nonce;
    void this.sendMessage({ connection_id: 0n, payload: { tag: "Ping", value: { nonce } } }).catch(() => {});

    // Expect a Pong within keepaliveTimeoutMs.
    this.keepalivePongTimer = setTimeout(() => {
      if (this.keepalivePendingNonce === nonce && !this.closed) {
        voxLogger()?.debug(
          `[vox:session] keepalive timeout — no Pong received, treating as dead connection`,
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

  async openConnection(
    settings: ConnectionSettings,
    metadata: Metadata = emptyMetadata(),
  ): Promise<ConnectionHandle> {
    // r[impl connection]
    // r[impl connection.virtual]
    // r[impl connection.open]
    // r[impl rpc.virtual-connection.open]
    this.assertOpen();
    if (settings.initial_channel_credit <= 0) {
      throw SessionError.protocol("initial_channel_credit must be greater than zero");
    }

    const connectionId = this.allocateConnectionId();
    const result = deferred<ConnectionHandle>();
    this.pendingConnections.set(connectionId, {
      localSettings: settings,
      result,
    });

    try {
      await this.sendMessage(messageConnect(connectionId, settings, metadata));
    } catch (error) {
      this.pendingConnections.delete(connectionId);
      throw error;
    }

    return result.promise;
  }

  async closeConnection(connectionId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    // r[impl connection]
    // r[impl connection.virtual]
    // r[impl connection.close]
    // r[impl connection.close.semantics]
    this.assertOpen();
    if (connectionId === 0n) {
      throw new SessionError("cannot close root connection");
    }

    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw SessionError.protocol(`unknown connection ${connectionId}`);
    }

    connection.close(SessionError.closed());
    this.connections.delete(connectionId);
    await this.sendMessage(messageGoodbye(connectionId, metadata));
    this.maybeShutdownAfterRootInternalClose();
  }

  async sendMessage(message: Message): Promise<void> {
    this.assertOpen();

    voxLogger()?.debug(
      `[vox:session] sendMessage: tag=${message.payload.tag} conn=${message.connection_id}`,
    );
    const op = this.sendChain.then(() => this.conduit.send(message));
    this.sendChain = op.then(() => undefined, () => undefined);
    await op;
  }

  fail(error: SessionError): void {
    if (this.closed) {
      return;
    }

    this.clearKeepaliveTimers();
    this.closed = true;
    this.closeError = error;
    this.conduit.close();

    for (const pending of this.pendingConnections.values()) {
      pending.result.reject(error);
    }
    this.pendingConnections.clear();

    for (const connection of this.connections.values()) {
      connection.close(error);
    }
    this.connections.clear();
  }

  shutdown(): void {
    this.fail(SessionError.closed());
  }

  callerLivenessDropped(connectionId: bigint): void {
    if (connectionId === 0n) {
      // r[impl rpc.caller.liveness.root-internal-close]
      this.rootInternallyClosed = true;
      // r[impl rpc.caller.liveness.root-teardown-condition]
      this.maybeShutdownAfterRootInternalClose();
      return;
    }

    // r[impl rpc.caller.liveness.last-drop-closes-connection]
    void this.closeConnection(connectionId).catch((error) => {
      if (!this.closed) {
        this.fail(error instanceof SessionError ? error : new SessionError(String(error)));
      }
    });
  }

  private maybeShutdownAfterRootInternalClose(): void {
    if (!this.rootInternallyClosed || this.closed) {
      return;
    }
    for (const connectionId of this.connections.keys()) {
      if (connectionId !== 0n) {
        return;
      }
    }
    this.shutdown();
  }

  private assertOpen(): void {
    if (this.closed) {
      throw this.closeError ?? SessionError.closed();
    }
  }

  private allocateConnectionId(): bigint {
    // r[impl session.parity]
    const id = this.nextConnectionId;
    this.nextConnectionId += 2n;
    return id;
  }

  private getConnection(connectionId: bigint): ConnectionHandle {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw SessionError.protocol(`unknown connection ${connectionId}`);
    }
    return connection;
  }

  private async run(): Promise<void> {
    // r[impl session.message]
    // r[impl session.message.connection-id]
    // r[impl session.message.payloads]
    while (!this.closed) {
      const message = await this.conduit.recv();
      if (!message) {
        this.clearKeepaliveTimers();
        this.failPendingAttempts(new SessionError("connection lost"));
        throw SessionError.closed();
      }
      await this.handleMessage(message);
    }
  }

  private async handleMessage(message: Message): Promise<void> {
    voxLogger()?.debug(`[vox:session] handleMessage: tag=${message.payload.tag} conn=${message.connection_id}`);
    switch (message.payload.tag) {
      case "Ping":
        // r[impl session.keepalive]
        void this.sendMessage({
          connection_id: 0n,
          payload: { tag: "Pong", value: { nonce: message.payload.value.nonce } },
        }).catch(() => {});
        return;

      case "Pong":
        // r[impl session.keepalive]
        if (this.keepalivePendingNonce === message.payload.value.nonce) {
          clearTimeout(this.keepalivePongTimer!);
          this.keepalivePongTimer = null;
          this.keepalivePendingNonce = null;
          this.scheduleKeepalive();
        }
        return;

      case "ProtocolError":
        throw SessionError.peerProtocol(message.payload.value.description);

      case "ConnectionOpen":
        await this.handleConnectionOpen(message.connection_id, message.payload.value);
        return;

      case "ConnectionAccept":
        this.handleConnectionAccept(message.connection_id, message.payload.value.connection_settings);
        return;

      case "ConnectionReject":
        this.handleConnectionReject(message.connection_id);
        return;

      case "ConnectionClose":
        this.handleConnectionClose(message.connection_id);
        return;

      case "RequestMessage":
        await this.handleRequestMessage(message.connection_id, message.payload.value);
        return;

      case "SchemaMessage":
        this.handleSchemaMessage(message.connection_id, message.payload.value);
        return;

      case "ChannelMessage":
        this.handleChannelMessage(message.connection_id, message.payload.value);
        return;

    }
  }

  private handleSchemaMessage(
    connectionId: bigint,
    schemaMessage: SchemaMessage,
  ): void {
    const connection = this.getConnection(connectionId);
    const direction = schemaMessage.direction.tag === "Args" ? "args" : "response";
    try {
      // r[impl schema.tracking.received]
      connection.getSchemaTracker().recordReceived(
        schemaMessage.method_id,
        direction,
        new Uint8Array(schemaMessage.schemas),
      );
    } catch (error) {
      throw SessionError.protocol(error instanceof Error ? error.message : String(error));
    }
  }

  // r[impl session.protocol-error]
  private async sendProtocolError(description: string): Promise<void> {
    try {
      await this.conduit.send(messageProtocolError(description));
    } catch (error) {
      voxLogger()?.debug("[vox:session] failed to send protocol error", error);
    }
  }

  private async handleConnectionOpen(
    connectionId: bigint,
    value: { connection_settings: ConnectionSettings; metadata: unknown },
  ): Promise<void> {
    // r[impl connection]
    // r[impl connection.virtual]
    // r[impl connection.open]
    // r[impl rpc.virtual-connection.accept]
    // r[impl connection.open.rejection]
    // r[impl session.connection-settings.open]
    if (!this.onConnection) {
      await this.sendMessage({
        connection_id: connectionId,
        payload: { tag: "ConnectionReject", value: { metadata: emptyMetadata() } },
      });
      return;
    }

    if (value.connection_settings.initial_channel_credit <= 0) {
      await this.sendMessage({
        connection_id: connectionId,
        payload: { tag: "ConnectionReject", value: { metadata: emptyMetadata() } },
      });
      return;
    }

    const localSettings: ConnectionSettings = {
      // r[impl connection.parity]
      parity: oppositeParity(value.connection_settings.parity),
      max_concurrent_requests: value.connection_settings.max_concurrent_requests,
      initial_channel_credit: value.connection_settings.initial_channel_credit,
    };
    const connection = new ConnectionHandle(
      this,
      connectionId,
      localSettings,
      value.connection_settings,
    );
    this.connections.set(connectionId, connection);
    await this.sendMessage(messageAccept(connectionId, localSettings, emptyMetadata()));
    void this.onConnection(connection);
  }

  private handleConnectionAccept(
    connectionId: bigint,
    peerSettings: ConnectionSettings,
  ): void {
    const pending = this.pendingConnections.get(connectionId);
    if (!pending) {
      return;
    }
    this.pendingConnections.delete(connectionId);
    const connection = new ConnectionHandle(
      this,
      connectionId,
      pending.localSettings,
      peerSettings,
    );
    this.connections.set(connectionId, connection);
    pending.result.resolve(connection);
  }

  private handleConnectionReject(connectionId: bigint): void {
    const pending = this.pendingConnections.get(connectionId);
    if (!pending) {
      return;
    }
    this.pendingConnections.delete(connectionId);
    pending.result.reject(new SessionError(`connection ${connectionId} rejected`));
  }

  private handleConnectionClose(connectionId: bigint): void {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      return;
    }
    connection.close(SessionError.closed());
    this.connections.delete(connectionId);
    this.maybeShutdownAfterRootInternalClose();
  }

  private async handleRequestMessage(
    connectionId: bigint,
    request: RequestMessage,
  ): Promise<void> {
    // r[impl rpc]
    // r[impl rpc.request]
    // r[impl rpc.request.id-allocation]
    // r[impl rpc.response]
    // r[impl rpc.cancel]
    // r[impl rpc.cancel.channels]
    // r[impl rpc.pipelining]
    const connection = this.getConnection(connectionId);
    switch (request.body.tag) {
      case "Call": {
        const callSchemas = request.body.value.schemas;
        if (callSchemas && callSchemas.length > 0) {
          try {
            // r[impl schema.tracking.received]
            connection.getSchemaTracker().recordReceived(
              request.body.value.method_id,
              "args",
              new Uint8Array(callSchemas),
            );
          } catch (error) {
            throw SessionError.protocol(error instanceof Error ? error.message : String(error));
          }
        }
        try {
          // r[impl schema.exchange.required]
          connection.getSchemaTracker().requireReceived(request.body.value.method_id, "args");
        } catch (error) {
          throw SessionError.protocol(error instanceof Error ? error.message : String(error));
        }
        connection.beginIncomingRequest(request.id);
        connection.enqueueIncomingCall({
          requestId: request.id,
          methodId: request.body.value.method_id,
          args: request.body.value.args,
          channels: request.body.value.channels,
          metadata: coerceMetadata(request.body.value.metadata),
          connectionEpoch: connection.currentEpoch(),
        });
        return;
      }

      case "Response": {
        const methodId = connection.pendingResponseMethodId(request.id);
        const responseSchemas = request.body.value.schemas;
        if (methodId !== undefined && responseSchemas && responseSchemas.length > 0) {
          try {
            // r[impl schema.tracking.received]
            connection.getSchemaTracker().recordReceived(
              methodId,
              "response",
              new Uint8Array(responseSchemas),
            );
          } catch (error) {
            throw SessionError.protocol(error instanceof Error ? error.message : String(error));
          }
        }
        if (methodId !== undefined) {
          try {
            // r[impl schema.exchange.required]
            connection.getSchemaTracker().requireReceived(methodId, "response");
          } catch (error) {
            throw SessionError.protocol(error instanceof Error ? error.message : String(error));
          }
        }
        connection.resolveResponse(request.id, request.body.value.ret);
        return;
      }

      case "Cancel":
        connection.enqueueIncomingCancel(request.id);
        return;
    }
  }

  private handleChannelMessage(
    connectionId: bigint,
    channel: ChannelMessage,
  ): void {
    // r[impl rpc.channel.item]
    // r[impl rpc.channel.close]
    // r[impl rpc.channel.connection-closure]
    // r[impl rpc.channel.reset]
    // r[impl rpc.flow-control]
    // r[impl rpc.flow-control.credit.grant]
    const connection = this.getConnection(connectionId);
    switch (channel.body.tag) {
      case "Item":
        connection.routeChannelData(channel.id, channel.body.value.item);
        return;

      case "Close":
      case "Reset":
        connection.closeChannel(channel.id);
        return;

      case "GrantCredit":
        connection.grantChannelCredit(channel.id, channel.body.value.additional);
        return;
    }
  }

}

export class SessionHandle {
  private readonly core: SessionCore;

  constructor(core: SessionCore) {
    this.core = core;
  }

  openConnection(
    settings: ConnectionSettings = this.core.defaultConnectionSettings(),
    metadata: Metadata = emptyMetadata(),
  ): Promise<ConnectionHandle> {
    return this.core.openConnection(settings, metadata);
  }

  closeConnection(connectionId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    return this.core.closeConnection(connectionId, metadata);
  }

  shutdown(): void {
    this.core.shutdown();
  }

  closed(): Promise<void> {
    return this.core.closedPromise();
  }
}

export class ConnectionHandle {
  private readonly role: Role;
  private readonly channelAllocator: ChannelIdAllocator;
  private readonly channelRegistry: ChannelRegistry;
  private readonly incomingCalls = new AsyncQueue<IncomingCall>();
  private readonly incomingCancels = new AsyncQueue<bigint>();
  private readonly pendingResponses = new Map<bigint, PendingResponse>();
  private readonly inboundLiveRequests = new Set<bigint>();
  private readonly outboundRequestLimit: AsyncSemaphore;
  private readonly schemaTracker = new SchemaTracker();
  private readonly schemaSendTracker = new SchemaSendTracker();
  private epoch = 0;
  private nextRequestId: bigint;
  private closed = false;
  private flushPromise: Promise<void> | null = null;
  private flushRequested = false;
  private callerRefs = 0;

  private readonly session: SessionCore;
  readonly id: bigint;
  readonly localSettings: ConnectionSettings;
  readonly peerSettings: ConnectionSettings;

  constructor(
    session: SessionCore,
    id: bigint,
    localSettings: ConnectionSettings,
    peerSettings: ConnectionSettings,
  ) {
    this.session = session;
    this.id = id;
    this.localSettings = localSettings;
    this.peerSettings = peerSettings;
    // r[impl rpc.flow-control.max-concurrent-requests]
    // r[impl rpc.flow-control.max-concurrent-requests.outbound]
    this.outboundRequestLimit = new AsyncSemaphore(peerSettings.max_concurrent_requests);
    // r[impl connection.parity]
    this.role = roleFromParity(localSettings.parity);
    this.channelAllocator = new ChannelIdAllocator(this.role);
    this.channelRegistry = new ChannelRegistry(undefined, () => {
      void this.flushOutgoing();
    });
    // r[impl connection.parity]
    this.nextRequestId = firstIdForParity(localSettings.parity);
  }

  sessionHandle(): SessionHandle {
    return this.session.sessionHandleValue();
  }

  caller(): Caller {
    return new ConnectionHandleCaller(this, this.retainCaller());
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

  // r[impl rpc.debug.snapshot]
  // r[impl rpc.observability.channel.context]
  debugSnapshot(): ConnectionDebugSnapshot {
    return {
      connectionId: this.id,
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
      throw SessionError.closed();
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
      responsePayload = await new Promise<Uint8Array>((resolve, reject) => {
        // The args schema closure is advertised once per (method, connection).
        const computeSchemas: () => number[] = () =>
          this.getSchemaSendTracker().prepareSchemas(
            request.descriptor.id,
            "args",
            methodSchemas.argsSchemaClosure,
          );

        const state: PendingResponse = {
          resolve,
          reject,
          timer: setTimeout(() => {
            this.clearPendingState(state);
            reject(new SessionError("timeout waiting for response"));
          }, request.timeoutMs ?? DEFAULT_TIMEOUT_MS),
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
    // schema binding. The session receive path enforces that the binding arrived
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
      case "ConnectionClosed":
      case "SessionShutdown":
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
      await this.session.sendMessage(messageResponse(requestId, payload, metadata, this.id, schemas));
    } finally {
      this.finishIncomingRequest(requestId);
    }
  }

  async sendCancel(requestId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    await this.session.sendMessage(messageCancel(requestId, this.id, metadata));
  }

  async sendChannelData(channelId: bigint, payload: Uint8Array): Promise<void> {
    await this.session.sendMessage(messageData(channelId, payload, this.id));
  }

  async sendChannelClose(channelId: bigint, metadata: Metadata = emptyMetadata()): Promise<void> {
    await this.session.sendMessage(messageClose(channelId, this.id, metadata));
  }

  async sendChannelCredit(channelId: bigint, additional: number): Promise<void> {
    await this.session.sendMessage(messageCredit(channelId, additional, this.id));
  }

  async sendSchemas(
    methodId: bigint,
    direction: "args" | "response",
    schemas: Uint8Array,
  ): Promise<void> {
    await this.session.sendMessage(messageSchema(methodId, direction, Array.from(schemas), this.id));
  }

  enqueueIncomingCall(call: IncomingCall): void {
    this.incomingCalls.push(call);
  }

  beginIncomingRequest(requestId: bigint): void {
    // r[impl rpc.flow-control.max-concurrent-requests]
    // r[impl rpc.flow-control.max-concurrent-requests.inbound]
    if (this.inboundLiveRequests.has(requestId)) {
      throw SessionError.protocol(`duplicate live request ${requestId}`);
    }
    if (this.inboundLiveRequests.size >= this.localSettings.max_concurrent_requests) {
      throw SessionError.protocol(
        `max_concurrent_requests exceeded for connection ${this.id}`,
      );
    }
    this.inboundLiveRequests.add(requestId);
  }

  finishIncomingRequest(requestId: bigint): void {
    // r[impl rpc.flow-control.max-concurrent-requests.counting]
    this.inboundLiveRequests.delete(requestId);
  }

  nextIncomingCall(): Promise<IncomingCall | null> {
    return this.incomingCalls.shift();
  }

  enqueueIncomingCancel(requestId: bigint): void {
    this.finishIncomingRequest(requestId);
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
      `[vox:session] resolveResponse: req=${requestId} payload=${payload.length}`,
    );
    this.clearPendingState(state, { finalizeChannels: false });
    state.resolve(payload);
  }

  pendingResponseMethodId(requestId: bigint): bigint | undefined {
    return this.pendingResponses.get(requestId)?.methodId;
  }

  routeChannelData(channelId: bigint, payload: Uint8Array): void {
    try {
      this.channelRegistry.routeData(channelId, payload);
    } catch (error) {
      if (error instanceof ChannelError) {
        this.close(new SessionError(error.message));
        return;
      }
      throw error;
    }
  }

  closeChannel(channelId: bigint): void {
    this.channelRegistry.close(channelId);
  }

  grantChannelCredit(channelId: bigint, additional: number): void {
    this.channelRegistry.grantCredit(channelId, additional);
  }

  close(error: Error): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    this.incomingCalls.close();
    this.incomingCancels.close();
    this.channelRegistry.closeAll();
    this.inboundLiveRequests.clear();
    // r[impl rpc.flow-control.max-concurrent-requests.session-failure]
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
    this.channelRegistry.closeAll();
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
    this.callerRefs += 1;
    let released = false;
    return () => {
      if (released) {
        return;
      }
      released = true;
      this.callerRefs -= 1;
      if (this.callerRefs === 0 && !this.closed) {
        this.session.callerLivenessDropped(this.id);
      }
    };
  }

  private clearPendingState(
    state: PendingResponse,
    options: { finalizeChannels?: boolean } = {},
  ): void {
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
      state.finalizeChannels?.();
    }
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
        `[vox:session] sendPendingRequest: req=${requestId} method=${state.methodId} payload=${state.payload.length} channels=${state.channels.length} schemas=${schemas?.length ?? 0}`,
      );
      await this.session.sendMessage(
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
      await this.flushOutgoing();
    } catch (error) {
      state.requestIds.delete(requestId);
      this.pendingResponses.delete(requestId);
      if (!rejectOnFailure || state.settled) {
        return;
      }
      this.clearPendingState(state);
      state.reject(error instanceof Error ? error : new SessionError(String(error)));
    }
  }

  private allocateRequestId(): bigint {
    // r[impl connection.parity]
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
        connectionId: this.id,
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

class ConnectionHandleCaller implements Caller {
  private readonly connection: ConnectionHandle;
  private readonly releaseCaller: () => void;
  private disposed = false;

  constructor(connection: ConnectionHandle, releaseCaller: () => void) {
    this.connection = connection;
    this.releaseCaller = releaseCaller;
  }

  call(request: CallerRequest): Promise<unknown> {
    return this.connection.call(request);
  }

  getChannelAllocator(): ChannelIdAllocator {
    return this.connection.getChannelAllocator();
  }

  getChannelRegistry(): ChannelRegistry {
    return this.connection.getChannelRegistry();
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

export class Session {
  private readonly core: SessionCore;

  private constructor(core: SessionCore) {
    this.core = core;
  }

  static initiatorConduit(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    options: SessionBuilderOptions = {},
  ): Session {
    const core = new SessionCore(
      conduit,
      handshake.localSettings,
      handshake.peerSettings,
      options.onConnection,
      options.keepaliveIntervalMs ?? 0,
      options.keepaliveTimeoutMs ?? 0,
    );
    core.rootConnection();
    core.start();
    return new Session(core);
  }



  rootConnection(): ConnectionHandle {
    return this.core.rootConnection();
  }

  handle(): SessionHandle {
    return this.core.sessionHandleValue();
  }

  closed(): Promise<void> {
    return this.core.closedPromise();
  }
}

export const session = {
  async initiator(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const established = await makeInitiatorEstablishedTransport(transport, options);
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      options,
    );
  },

  initiatorConduit(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    options: SessionBuilderOptions = {},
  ): Session {
    return Session.initiatorConduit(conduit, handshake, options);
  },

  acceptorConduit(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    options: SessionBuilderOptions = {},
  ): Session {
    return Session.initiatorConduit(conduit, handshake, options);
  },

  async initiatorOnLink(
    link: Link,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
      initial_channel_credit: channelCapacityFromOptions(options),
    };
    const handshake = await handshakeAsInitiator(
      link,
      localSettings,
      options.metadata ?? emptyMetadata(),
    );
    return Session.initiatorConduit(
      new BareConduit(link, buildMessageDecodePlan(handshake.peerMessageSchema)),
      handshake,
      options,
    );
  },

  async acceptorOnLink(
    link: Link,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
      initial_channel_credit: channelCapacityFromOptions(options),
    };
    const handshake = await handshakeAsAcceptor(
      link,
      localSettings,
      options.metadata ?? emptyMetadata(),
    );
    return Session.initiatorConduit(
      new BareConduit(link, buildMessageDecodePlan(handshake.peerMessageSchema)),
      handshake,
      options,
    );
  },

  async initiatorOn(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const established = await makeInitiatorEstablishedTransport(transport, options);
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      options,
    );
  },

  async acceptorOn(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const established = await makeAcceptorEstablishedTransport(transport, options);
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      options,
    );
  },

  async initiatorTransport(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    return session.initiator(transport, options);
  },

  async acceptorTransport(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const established = await makeAcceptorEstablishedTransport(transport, options);
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      options,
    );
  },

  rootSettings(
    role: Role,
    maxConcurrentRequests = 64,
    channelCapacity = DEFAULT_CHANNEL_CAPACITY,
  ): ConnectionSettings {
    // r[impl rpc.flow-control.credit.initial.high-level]
    // r[impl rpc.flow-control.credit.initial.zero]
    if (channelCapacity <= 0) {
      throw SessionError.protocol("initial_channel_credit must be greater than zero");
    }

    return {
      // r[impl session.parity]
      parity: parityFromRole(role),
      // r[impl rpc.flow-control.max-concurrent-requests.default]
      max_concurrent_requests: maxConcurrentRequests,
      initial_channel_credit: channelCapacity,
    };
  },
};
