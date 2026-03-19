import { decodeWithSchema, encodeWithSchema } from "@bearcove/roam-postcard";
import {
  type ConnectionSettings,
  type RequestMessage,
  type ChannelMessage,
  type Message,
  type Metadata,
  type MetadataEntry,
  parityEven,
  parityOdd,
  messageAccept,
  messageConnect,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageCredit,
  RpcError,
  RpcErrorCode,
} from "@bearcove/roam-wire";
import {
  ChannelError,
  ChannelIdAllocator,
  ChannelRegistry,
  Role,
} from "./channeling/index.ts";
import type { Caller, CallerRequest } from "./caller.ts";
import { MiddlewareCaller } from "./caller.ts";
import type { ClientMiddleware } from "./middleware.ts";
import { ClientMetadata, clientMetadataToEntries } from "./metadata.ts";
import type { Conduit } from "./conduit.ts";
import { AsyncQueue } from "./internal/async_queue.ts";
import { deferred, type Deferred } from "./internal/deferred.ts";
import {
  appendRetrySupportMetadata,
  ensureOperationId,
  metadataSupportsRetry,
} from "./retry.ts";
import {
  appendSessionResumeKeyMetadata,
  metadataSessionResumeKey,
} from "./session_resume.ts";
import {
  firstIdForParity,
  oppositeParity,
  parityFromRole,
  roleFromParity,
} from "./internal/parity.ts";
import { BareConduit } from "./conduit.ts";
import { roamLogger } from "./logger.ts";
import { SchemaTracker, SchemaSendTracker } from "./schema_tracker.ts";
import type { Link, LinkSource } from "./link.ts";
import { singleLinkSource } from "./link.ts";
import { StableConduit } from "./stable_conduit.ts";
import {
  acceptTransportMode,
  requestTransportMode,
} from "./transport_prologue.ts";
import {
  handshakeAsAcceptor,
  handshakeAsInitiator,
  type HandshakeResult,
} from "./handshake.ts";

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
   * Called on every send so that after a SchemaSendTracker.reset() (session
   * resume), fresh schemas are included in the retried request.
   */
  computeSchemas?: () => Uint8Array;
  /** Whether this method uses persist retry policy (affects close behavior). */
  persist: boolean;
  /** Whether this method is idempotent. */
  idem: boolean;
  prepareRetry?: () => {
    payload: Uint8Array;
    channels: bigint[];
  };
  finalizeChannels?: () => void;
  requestIds: Set<bigint>;
  settled: boolean;
}

export interface IncomingCall {
  requestId: bigint;
  methodId: bigint;
  args: Uint8Array;
  channels: bigint[];
  metadata: MetadataEntry[];
}

/** Current connectivity state of a resumable session. */
export type SessionConnectivity =
  | "connected"
  | "disconnected"
  | "reconnecting"
  | "failed";

/** Policy controlling how a resumable session retries after a connection drop. */
export interface ReconnectPolicy {
  /**
   * Maximum number of reconnect attempts before the session is permanently
   * failed. Defaults to 10. Set to Infinity to retry indefinitely.
   */
  maxAttempts?: number;
  /**
   * Base delay in ms for the first retry. Subsequent retries use exponential
   * backoff: baseDelay * 2^(attempt-1), capped at maxDelay.
   * Defaults to 500 ms.
   */
  baseDelay?: number;
  /**
   * Maximum delay in ms between retries. Defaults to 30_000 (30 s).
   */
  maxDelay?: number;
}

export interface SessionBuilderOptions {
  maxConcurrentRequests?: number;
  metadata?: Metadata;
  onConnection?: (connection: ConnectionHandle) => void | Promise<void>;
  resumable?: boolean;
  /** Reconnect retry policy for resumable sessions. */
  reconnect?: ReconnectPolicy;
  /**
   * If set, the session sends a Ping every `keepaliveIntervalMs` milliseconds
   * and expects a Pong back within `keepaliveTimeoutMs` (default: half the
   * interval). If no Pong arrives in time the connection is considered dead
   * and session recovery begins. Set to 0 or undefined to disable keepalive.
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
  /**
   * Called when the transport drops and the session enters a reconnecting
   * state. Receives the attempt number (1-based).
   */
  onReconnecting?: (attempt: number) => void;
  /**
   * Called after a successful reconnect. The session continues normally.
   */
  onReconnected?: () => void;
  /**
   * Called when the session gives up reconnecting (all attempts exhausted).
   */
  onReconnectFailed?: (error: Error) => void;
  /**
   * Called whenever the session connectivity changes.
   */
  onConnectivityChange?: (state: SessionConnectivity) => void;
}

export class SessionRegistry {
  private readonly sessions = new Map<string, SessionHandle>();

  get(key: Uint8Array): SessionHandle | undefined {
    return this.sessions.get(sessionResumeKeyId(key));
  }

  insert(key: Uint8Array, handle: SessionHandle): void {
    this.sessions.set(sessionResumeKeyId(key), handle);
  }

  remove(key: Uint8Array): void {
    this.sessions.delete(sessionResumeKeyId(key));
  }
}

export type SessionAcceptOutcome =
  | { tag: "Established"; session: Session }
  | { tag: "Resumed" };

export type SessionConduitKind = "bare" | "stable";

export interface SessionTransportOptions extends SessionBuilderOptions {
  transport?: SessionConduitKind;
  conduit?: SessionConduitKind;
}

type SessionTransport = Link | LinkSource;

function isLinkSource(value: SessionTransport): value is LinkSource {
  return typeof (value as LinkSource).nextLink === "function";
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) {
    return false;
  }
  for (let i = 0; i < left.length; i++) {
    if (left[i] !== right[i]) {
      return false;
    }
  }
  return true;
}

function sessionResumeKeyId(key: Uint8Array): string {
  return Array.from(key, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function cloneMetadataValue(value: MetadataEntry["value"]): MetadataEntry["value"] {
  switch (value.tag) {
    case "Bytes":
      return { tag: "Bytes", value: value.value.slice() };
    case "String":
      return { tag: "String", value: value.value };
    case "U64":
      return { tag: "U64", value: value.value };
  }
}

function cloneMetadata(metadata: Metadata): Metadata {
  return metadata.map((entry) => ({
    key: entry.key,
    value: cloneMetadataValue(entry.value),
    flags: entry.flags,
  }));
}

interface EstablishedTransport {
  conduit: Conduit<Message>;
  handshake: HandshakeResult;
  recoverConduit?: () => Promise<Conduit<Message>>;
}

async function makeInitiatorEstablishedTransport(
  transport: SessionTransport,
  options: SessionTransportOptions,
): Promise<EstablishedTransport> {
  const conduitKind = options.transport ?? options.conduit ?? "bare";
  const localSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
  };

  if (isLinkSource(transport)) {
    const attachment = await transport.nextLink();
    await requestTransportMode(attachment.link, conduitKind);
    const handshake = await handshakeAsInitiator(attachment.link, localSettings, true, null);

    if (conduitKind === "stable") {
      const stableConduit = await StableConduit.connect(singleLinkSource(attachment.link));
      return { conduit: stableConduit, handshake };
    }

    // For resumable bare sessions: build a recoverConduit that reconnects,
    // re-handshakes with the stored resume key, and returns a fresh conduit.
    // Retries with exponential backoff according to the reconnect policy.
    if (options.resumable && handshake.sessionResumeKey) {
      // Use a mutable cell so recoverConduit always uses the latest key.
      const keyCell: { value: Uint8Array | null } = {
        value: handshake.sessionResumeKey,
      };

      const policy = options.reconnect ?? {};
      const maxAttempts = policy.maxAttempts ?? 10;
      const baseDelay = policy.baseDelay ?? 500;
      const maxDelay = policy.maxDelay ?? 30_000;

      const recoverConduit = async (): Promise<Conduit<Message>> => {
        let lastError: Error = new Error("unknown reconnect failure");

        for (let attempt = 1; attempt <= maxAttempts; attempt++) {
          options.onReconnecting?.(attempt);
          options.onConnectivityChange?.("reconnecting");
          roamLogger()?.debug(`[roam:session] reconnect attempt ${attempt}/${maxAttempts}`);

          try {
            const newAttachment = await (transport as LinkSource).nextLink();
            await requestTransportMode(newAttachment.link, conduitKind);
            const newHandshake = await handshakeAsInitiator(
              newAttachment.link,
              localSettings,
              true,
              keyCell.value,
            );
            // Update the key for the next reconnect.
            keyCell.value = newHandshake.sessionResumeKey;
            options.onReconnected?.();
            options.onConnectivityChange?.("connected");
            roamLogger()?.debug(`[roam:session] reconnect succeeded on attempt ${attempt}`);
            return new BareConduit(newAttachment.link);
          } catch (e) {
            lastError = e instanceof Error ? e : new Error(String(e));
            roamLogger()?.debug(
              `[roam:session] reconnect attempt ${attempt} failed: ${lastError.message}`,
            );

            if (attempt < maxAttempts) {
              const delay = Math.min(baseDelay * Math.pow(2, attempt - 1), maxDelay);
              roamLogger()?.debug(`[roam:session] retrying in ${delay}ms`);
              await new Promise<void>((resolve) => setTimeout(resolve, delay));
            }
          }
        }

        options.onReconnectFailed?.(lastError);
        options.onConnectivityChange?.("failed");
        roamLogger()?.error(`[roam:session] all ${maxAttempts} reconnect attempts failed`);
        throw lastError;
      };

      return {
        conduit: new BareConduit(attachment.link),
        handshake,
        recoverConduit,
      };
    }

    return {
      conduit: new BareConduit(attachment.link),
      handshake,
    };
  }

  await requestTransportMode(transport, conduitKind);
  const handshake = await handshakeAsInitiator(transport, localSettings, true, null);
  if (conduitKind === "stable") {
    const stableConduit = await StableConduit.connect(singleLinkSource(transport));
    return { conduit: stableConduit, handshake };
  }

  return {
    conduit: new BareConduit(transport),
    handshake,
  };
}

async function makeAcceptorEstablishedTransport(
  transport: SessionTransport,
  options: SessionTransportOptions,
): Promise<EstablishedTransport> {
  const attachment = isLinkSource(transport)
    ? await transport.nextLink()
    : { link: transport };
  const requestedMode = await acceptTransportMode(attachment.link);

  const localSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
  };

  const handshake = await handshakeAsAcceptor(
    attachment.link,
    localSettings,
    true,
    options.resumable ?? false,
    null,
  );

  if (requestedMode === "stable") {
    const clientHello = await attachment.link.recv();
    if (!clientHello) {
      throw SessionError.protocol("expected StableConduit ClientHello after CBOR handshake");
    }
    const stableConduit = await StableConduit.connect(
      singleLinkSource(attachment.link, clientHello),
    );
    return { conduit: stableConduit, handshake };
  }

  return {
    conduit: new BareConduit(attachment.link),
    handshake,
  };
}



export class SessionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SessionError";
  }

  static closed(): SessionError {
    return new SessionError("session closed");
  }

  static protocol(message: string): SessionError {
    return new SessionError(message);
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
  private disconnected = false;
  private closeError: SessionError | null = null;
  private rootConnectionValue: ConnectionHandle | null = null;
  private runPromise: Promise<void> | null = null;
  private peerSupportsRetry: boolean;
  private readonly resumable: boolean;
  private sessionResumeKey: Uint8Array | null;
  private readonly schemaTracker: SchemaTracker;
  private readonly schemaSendTracker = new SchemaSendTracker();
  private readonly recoverConduit?: () => Promise<Conduit<Message>>;
  private readonly onConnectivityChange?: (state: SessionConnectivity) => void;
  private readonly keepaliveIntervalMs: number;
  private readonly keepaliveTimeoutMs: number;
  private keepaliveTimer: ReturnType<typeof setTimeout> | null = null;
  private keepalivePendingNonce: bigint | null = null;
  private keepalivePongTimer: ReturnType<typeof setTimeout> | null = null;
  private nextKeepaliveNonce = 1n;
  private readonly pendingResumes: Array<{
    conduit: Conduit<Message>;
    result: Deferred<void>;
  }> = [];
  private resumeWaiter: ((request: { conduit: Conduit<Message>; result: Deferred<void> } | null) => void) | null = null;

  constructor(
    conduit: Conduit<Message>,
    private readonly localRootSettings: ConnectionSettings,
    private readonly peerRootSettings: ConnectionSettings,
    peerSupportsRetry: boolean,
    resumable: boolean,
    sessionResumeKey: Uint8Array | null,
    recoverConduit: (() => Promise<Conduit<Message>>) | undefined,
    private readonly onConnection?: (connection: ConnectionHandle) => void | Promise<void>,
    onConnectivityChange?: (state: SessionConnectivity) => void,
    keepaliveIntervalMs = 0,
    keepaliveTimeoutMs = 0,
  ) {
    this.conduit = conduit;
    this.peerSupportsRetry = peerSupportsRetry;
    this.resumable = resumable;
    this.sessionResumeKey = sessionResumeKey?.slice() ?? null;
    this.recoverConduit = recoverConduit;
    this.nextConnectionId = firstIdForParity(localRootSettings.parity);
    this.sessionHandle = new SessionHandle(this);
    this.schemaTracker = new SchemaTracker();
    this.onConnectivityChange = onConnectivityChange;
    this.keepaliveIntervalMs = keepaliveIntervalMs;
    this.keepaliveTimeoutMs =
      keepaliveTimeoutMs > 0 ? keepaliveTimeoutMs : Math.floor(keepaliveIntervalMs / 2);
  }

  sessionHandleValue(): SessionHandle {
    return this.sessionHandle;
  }

  sessionResumeKeyValue(): Uint8Array | null {
    return this.sessionResumeKey?.slice() ?? null;
  }

  defaultConnectionSettings(): ConnectionSettings {
    return {
      parity: this.localRootSettings.parity,
      max_concurrent_requests: this.localRootSettings.max_concurrent_requests,
    };
  }

  rootConnection(): ConnectionHandle {
    if (!this.rootConnectionValue) {
      this.rootConnectionValue = new ConnectionHandle(
        this,
        0n,
        this.localRootSettings,
        this.peerRootSettings,
        this.peerSupportsRetry,
      );
      this.connections.set(0n, this.rootConnectionValue);
    }
    return this.rootConnectionValue;
  }

  notifyConnectionsResumed(): void {
    for (const connection of this.connections.values()) {
      connection.onSessionResumed();
    }
  }

  start(): void {
    if (this.runPromise) {
      return;
    }
    this.runPromise = this.run().catch((error) => {
      roamLogger()?.error(`[roam:session] run loop error:`, error);
      this.fail(error instanceof SessionError ? error : new SessionError(String(error)));
    });
    if (this.keepaliveIntervalMs > 0) {
      this.scheduleKeepalive();
    }
  }

  private scheduleKeepalive(): void {
    if (this.closed || this.keepaliveIntervalMs <= 0) return;
    this.keepaliveTimer = setTimeout(() => {
      this.sendKeepalivePing();
    }, this.keepaliveIntervalMs);
  }

  private sendKeepalivePing(): void {
    if (this.closed || this.disconnected) {
      this.scheduleKeepalive();
      return;
    }
    const nonce = this.nextKeepaliveNonce++;
    this.keepalivePendingNonce = nonce;
    void this.sendMessage({ connection_id: 0n, payload: { tag: "Ping", value: { nonce } } }).catch(() => {});

    // Expect a Pong within keepaliveTimeoutMs.
    this.keepalivePongTimer = setTimeout(() => {
      if (this.keepalivePendingNonce === nonce && !this.closed) {
        roamLogger()?.debug(
          `[roam:session] keepalive timeout — no Pong received, treating as dead connection`,
        );
        this.keepalivePendingNonce = null;
        // Force the conduit closed so the run loop detects the drop.
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
    metadata: Metadata = [],
  ): Promise<ConnectionHandle> {
    // r[impl connection.open]
    this.assertConnected("resume before opening connections");
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

  async closeConnection(connectionId: bigint, metadata: Metadata = []): Promise<void> {
    // r[impl connection.close]
    this.assertConnected("resume before closing connections");
    if (connectionId === 0n) {
      throw new SessionError("cannot close root connection");
    }

    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw new SessionError(`unknown connection ${connectionId}`);
    }

    connection.close(SessionError.closed());
    this.connections.delete(connectionId);
    await this.sendMessage(messageGoodbye(connectionId, metadata));
  }

  async sendMessage(message: Message): Promise<void> {
    this.assertConnected("resume before sending");

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
    this.rejectPendingResumes(error);

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

  private assertOpen(): void {
    if (this.closed) {
      throw this.closeError ?? SessionError.closed();
    }
  }

  private assertConnected(reason: string): void {
    this.assertOpen();
    if (this.disconnected) {
      throw SessionError.protocol(`session is disconnected; ${reason}`);
    }
  }

  private allocateConnectionId(): bigint {
    const id = this.nextConnectionId;
    this.nextConnectionId += 2n;
    return id;
  }

  private getConnection(connectionId: bigint): ConnectionHandle {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw new SessionError(`unknown connection ${connectionId}`);
    }
    return connection;
  }

  private async run(): Promise<void> {
    // r[impl session.message]
    while (!this.closed) {
      const message = await this.conduit.recv();
      if (!message) {
        this.clearKeepaliveTimers();
        if (await this.handleConduitBreak()) {
          // Reconnected — restart keepalive on the fresh connection.
          if (this.keepaliveIntervalMs > 0) {
            this.scheduleKeepalive();
          }
          continue;
        }
        throw SessionError.closed();
      }
      await this.handleMessage(message);
    }
  }

  async resume(conduit: Conduit<Message>): Promise<void> {
    this.assertOpen();
    if (!this.resumable) {
      throw SessionError.protocol("session is not resumable");
    }
    if (!this.disconnected) {
      throw SessionError.protocol("resume is only valid while the session is disconnected");
    }
    const result = deferred<void>();
    this.enqueueResume({ conduit, result });
    await result.promise;
  }

  async acceptResumedConduit(conduit: Conduit<Message>): Promise<void> {
    this.assertOpen();
    if (!this.resumable) {
      throw SessionError.protocol("session is not resumable");
    }
    const result = deferred<void>();
    this.enqueueResume({ conduit, result });
    await result.promise;
  }

  private async handleConduitBreak(): Promise<boolean> {
    if (this.closed) {
      return false;
    }
    if (!this.resumable) {
      return false;
    }

    if (this.recoverConduit) {
      this.onConnectivityChange?.("disconnected");
      try {
        const conduit = await this.recoverConduit();
        if (this.closed) {
          conduit.close();
          return false;
        }
        this.disconnected = true;
        await this.resumeOnConduit(conduit);
        this.disconnected = false;
        this.notifyConnectionsResumed();
        return true;
      } catch {
        return false;
      }
    }

    this.disconnected = true;
    while (!this.closed) {
      const pending = await this.nextResume();
      if (!pending) {
        return false;
      }
      try {
        await this.resumeOnConduit(pending.conduit);
        this.disconnected = false;
        this.notifyConnectionsResumed();
        pending.result.resolve();
        return true;
      } catch (error) {
        pending.result.reject(
          error instanceof SessionError ? error : new SessionError(String(error)),
        );
      }
    }

    return false;
  }

  private async resumeOnConduit(conduit: Conduit<Message>): Promise<void> {
    if (!this.sessionResumeKey) {
      throw SessionError.protocol("session is not resumable");
    }

    // CBOR handshake (including resume key exchange) is performed by the
    // caller before the conduit is handed in. Just switch to the new conduit.
    this.conduit = conduit;
    // Reset the schema send tracker so schemas are re-sent on the resumed
    // connection. The Rust peer resets its receive tracker on resume, so we
    // must re-announce all schemas that were previously sent.
    this.schemaSendTracker.reset();
  }

  private enqueueResume(request: { conduit: Conduit<Message>; result: Deferred<void> }): void {
    const waiter = this.resumeWaiter;
    if (waiter) {
      this.resumeWaiter = null;
      waiter(request);
      return;
    }
    this.pendingResumes.push(request);
  }

  private async nextResume(): Promise<{ conduit: Conduit<Message>; result: Deferred<void> } | null> {
    const pending = this.pendingResumes.shift();
    if (pending) {
      return pending;
    }
    if (this.closed) {
      return null;
    }
    return new Promise((resolve) => {
      this.resumeWaiter = resolve;
    });
  }

  private rejectPendingResumes(error: SessionError): void {
    const waiter = this.resumeWaiter;
    this.resumeWaiter = null;
    waiter?.(null);
    for (const pending of this.pendingResumes.splice(0)) {
      pending.result.reject(error);
    }
  }

  private async handleMessage(message: Message): Promise<void> {
    roamLogger()?.debug(`[roam:session] handleMessage: tag=${message.payload.tag} conn=${message.connection_id}`);
    switch (message.payload.tag) {
      case "Ping":
        // Respond to peer's keepalive ping.
        void this.sendMessage({
          connection_id: 0n,
          payload: { tag: "Pong", value: { nonce: message.payload.value.nonce } },
        }).catch(() => {});
        return;

      case "Pong":
        // Acknowledge our own keepalive ping.
        if (this.keepalivePendingNonce === message.payload.value.nonce) {
          clearTimeout(this.keepalivePongTimer!);
          this.keepalivePongTimer = null;
          this.keepalivePendingNonce = null;
          // Schedule the next ping.
          this.scheduleKeepalive();
        }
        return;

      case "ProtocolError":
        throw SessionError.protocol(message.payload.value.description);

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

      case "ChannelMessage":
        this.handleChannelMessage(message.connection_id, message.payload.value);
        return;

    }
  }

  private async handleConnectionOpen(
    connectionId: bigint,
    value: { connection_settings: ConnectionSettings; metadata: Metadata },
  ): Promise<void> {
    // r[impl connection.open]
    // r[impl connection.open.rejection]
    // r[impl session.connection-settings.open]
    if (!this.onConnection) {
      await this.sendMessage({
        connection_id: connectionId,
        payload: { tag: "ConnectionReject", value: { metadata: [] } },
      });
      return;
    }

    const localSettings: ConnectionSettings = {
      parity: oppositeParity(value.connection_settings.parity),
      max_concurrent_requests: this.localRootSettings.max_concurrent_requests,
    };
    const connection = new ConnectionHandle(
      this,
      connectionId,
      localSettings,
      value.connection_settings,
      this.peerSupportsRetry,
    );
    this.connections.set(connectionId, connection);
    await this.sendMessage(messageAccept(connectionId, localSettings, []));
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
      this.peerSupportsRetry,
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
  }

  private async handleRequestMessage(
    connectionId: bigint,
    request: RequestMessage,
  ): Promise<void> {
    // r[impl rpc.request]
    // r[impl rpc.response]
    // r[impl rpc.cancel]
    const connection = this.getConnection(connectionId);
    switch (request.body.tag) {
      case "Call": {
        const callSchemas = request.body.value.schemas;
        if (callSchemas && callSchemas.length > 0) {
          try { this.schemaTracker.recordReceived(callSchemas); } catch {}
        }
        connection.enqueueIncomingCall({
          requestId: request.id,
          methodId: request.body.value.method_id,
          args: request.body.value.args,
          channels: request.body.value.channels,
          metadata: request.body.value.metadata,
        });
        return;
      }

      case "Response": {
        const responseSchemas = request.body.value.schemas;
        if (responseSchemas && responseSchemas.length > 0) {
          try { this.schemaTracker.recordReceived(responseSchemas); } catch {}
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
    // r[impl rpc.channel.reset]
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

  getSchemaTracker(): SchemaTracker {
    return this.schemaTracker;
  }

  getSchemaSendTracker(): SchemaSendTracker {
    return this.schemaSendTracker;
  }
}

export class SessionHandle {
  constructor(private readonly core: SessionCore) {}

  openConnection(
    settings: ConnectionSettings = this.core.defaultConnectionSettings(),
    metadata: Metadata = [],
  ): Promise<ConnectionHandle> {
    return this.core.openConnection(settings, metadata);
  }

  closeConnection(connectionId: bigint, metadata: Metadata = []): Promise<void> {
    return this.core.closeConnection(connectionId, metadata);
  }

  sessionResumeKey(): Uint8Array | null {
    return this.core.sessionResumeKeyValue();
  }

  resume(conduit: Conduit<Message>): Promise<void> {
    return this.core.resume(conduit);
  }

  acceptResumedConduit(conduit: Conduit<Message>): Promise<void> {
    return this.core.acceptResumedConduit(conduit);
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
  private nextRequestId: bigint;
  private nextOperationId = 1n;
  private closed = false;
  private flushPromise: Promise<void> | null = null;

  constructor(
    private readonly session: SessionCore,
    readonly id: bigint,
    readonly localSettings: ConnectionSettings,
    readonly peerSettings: ConnectionSettings,
    readonly peerSupportsRetry: boolean,
  ) {
    this.role = roleFromParity(localSettings.parity);
    this.channelAllocator = new ChannelIdAllocator(this.role);
    this.channelRegistry = new ChannelRegistry(undefined, () => {
      void this.flushOutgoing();
    });
    this.nextRequestId = firstIdForParity(localSettings.parity);
  }

  sessionHandle(): SessionHandle {
    return this.session.sessionHandleValue();
  }

  caller(): Caller {
    return new ConnectionHandleCaller(this);
  }

  getChannelAllocator(): ChannelIdAllocator {
    return this.channelAllocator;
  }

  getChannelRegistry(): ChannelRegistry {
    return this.channelRegistry;
  }

  getSchemaTracker(): SchemaTracker {
    return this.session.getSchemaTracker();
  }

  getSchemaSendTracker(): import("./schema_tracker.ts").SchemaSendTracker {
    return this.session.getSchemaSendTracker();
  }

  isClosed(): boolean {
    return this.closed;
  }

  async call(request: CallerRequest): Promise<unknown> {
    if (this.closed) {
      throw SessionError.closed();
    }

    const values = Object.values(request.args);
    const initial = request.prepareRetry
      ? request.prepareRetry()
      : {
          payload:
            values.length === 0
              ? new Uint8Array(0)
              : encodeWithSchema(values, request.descriptor.args, request.schemaRegistry),
          channels: request.channels ?? [],
        };
    const metadataCarrier = request.metadata ? request.metadata.clone() : new ClientMetadata();
    if (this.peerSupportsRetry) {
      ensureOperationId(metadataCarrier, this.nextOperationId++);
    }
    const metadata = clientMetadataToEntries(metadataCarrier);
    const requestId = this.nextRequestId;
    this.nextRequestId += 2n;
    const responsePayload = await new Promise<Uint8Array>((resolve, reject) => {
      // Build a lazy schema computer so each send attempt (including retries
      // after session reconnect) can call into the tracker which may have
      // been reset. On first call the tracker returns the full schema bytes;
      // on subsequent calls within the same connection it returns empty bytes.
      const computeSchemas: (() => Uint8Array) | undefined = request.sendSchemas
        ? () =>
            this.session.getSchemaSendTracker().prepareSchemas(
              request.descriptor.id,
              "args",
              request.sendSchemas!,
            )
        : undefined;

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
        persist: request.descriptor.retry?.persist ?? false,
        idem: request.descriptor.retry?.idem ?? false,
        prepareRetry: request.prepareRetry,
        finalizeChannels: request.finalizeChannels,
        requestIds: new Set(),
        settled: false,
      };

      this.pendingResponses.set(requestId, state);
      state.requestIds.add(requestId);

      void this.sendPendingRequest(state, requestId, true);
    });

    const decoded = decodeWithSchema(
      responsePayload,
      0,
      request.descriptor.result,
      request.schemaRegistry,
    ).value as { tag: string; value?: unknown };

    if (decoded.tag === "Ok") {
      return decoded.value;
    }

    const err = decoded.value as { tag: string; value?: unknown };
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
    metadata: Metadata = [],
    channels: bigint[] = [],
    schemas: Uint8Array = new Uint8Array(0),
  ): Promise<void> {
    await this.session.sendMessage(messageResponse(requestId, payload, metadata, channels, this.id, schemas));
  }

  async sendCancel(requestId: bigint, metadata: Metadata = []): Promise<void> {
    await this.session.sendMessage(messageCancel(requestId, this.id, metadata));
  }

  async sendChannelData(channelId: bigint, payload: Uint8Array): Promise<void> {
    await this.session.sendMessage(messageData(channelId, payload, this.id));
  }

  async sendChannelClose(channelId: bigint, metadata: Metadata = []): Promise<void> {
    await this.session.sendMessage(messageClose(channelId, this.id, metadata));
  }

  async sendChannelCredit(channelId: bigint, additional: number): Promise<void> {
    await this.session.sendMessage(messageCredit(channelId, additional, this.id));
  }

  enqueueIncomingCall(call: IncomingCall): void {
    this.incomingCalls.push(call);
  }

  nextIncomingCall(): Promise<IncomingCall | null> {
    return this.incomingCalls.shift();
  }

  enqueueIncomingCancel(requestId: bigint): void {
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
    this.clearPendingState(state);
    state.resolve(payload);
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
      // Report INDETERMINATE when session closes for:
      //   - persist=true methods (op may have executed on remote)
      //   - channel-bearing non-idempotent methods (fails-closed: channel
      //     state is indeterminate after a broken connection)
      const failClosedOnDrop = pending.channels.length > 0 && !pending.idem;
      if ((pending.persist || failClosedOnDrop) && error instanceof SessionError) {
        pending.reject(new RpcError(RpcErrorCode.INDETERMINATE));
      } else {
        pending.reject(error);
      }
    }
  }

  onSessionResumed(): void {
    if (this.closed || !this.peerSupportsRetry) {
      return;
    }
    this.channelRegistry.closeAll();
    const states = new Set(this.pendingResponses.values());
    for (const state of states) {
      if (state.settled) {
        continue;
      }
      for (const requestId of state.requestIds) {
        this.pendingResponses.delete(requestId);
      }
      state.requestIds.clear();
      void this.sendPendingRequest(state);
    }
  }

  private clearPendingState(state: PendingResponse): void {
    if (state.settled) {
      return;
    }
    state.settled = true;
    clearTimeout(state.timer);
    for (const requestId of state.requestIds) {
      this.pendingResponses.delete(requestId);
    }
    state.requestIds.clear();
    state.finalizeChannels?.();
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
      if (state.prepareRetry) {
        const rebuilt = state.prepareRetry();
        state.payload = rebuilt.payload.slice();
        state.channels = [...rebuilt.channels];
      }
      await this.session.sendMessage(
        messageRequest(
          requestId,
          state.methodId,
          state.payload,
          cloneMetadata(state.metadata),
          [...state.channels],
          this.id,
          state.computeSchemas?.(),
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
    const requestId = this.nextRequestId;
    this.nextRequestId += 2n;
    return requestId;
  }

  async flushOutgoing(): Promise<void> {
    if (this.closed) {
      return;
    }
    if (this.flushPromise) {
      await this.flushPromise;
      return;
    }

    const flush = (async () => {
      while (!this.closed) {
        const poll = this.channelRegistry.pollOutgoing();
        if (poll.kind === "pending" || poll.kind === "done") {
          return;
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
  constructor(private readonly connection: ConnectionHandle) {}

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
}

export class Session {
  private constructor(private readonly core: SessionCore) {}

  private resumeKey(): Uint8Array | null {
    return this.core.sessionResumeKeyValue();
  }

  static initiatorConduit(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    options: SessionBuilderOptions = {},
    recoverConduit?: () => Promise<Conduit<Message>>,
  ): Session {
    if (options.resumable && !handshake.sessionResumeKey) {
      throw SessionError.protocol("peer did not advertise session resumption");
    }
    const core = new SessionCore(
      conduit,
      handshake.localSettings,
      handshake.peerSettings,
      handshake.peerSupportsRetry,
      options.resumable ?? false,
      handshake.sessionResumeKey,
      recoverConduit,
      options.onConnection,
      options.onConnectivityChange,
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

class PrefetchedConduit implements Conduit<Message> {
  private first: Message | null;

  constructor(
    first: Message,
    private readonly inner: Conduit<Message>,
  ) {
    this.first = first;
  }

  send(item: Message): Promise<void> {
    return this.inner.send(item);
  }

  async recv(): Promise<Message | null> {
    if (this.first) {
      const first = this.first;
      this.first = null;
      return first;
    }
    return this.inner.recv();
  }

  close(): void {
    this.inner.close();
  }

  isClosed(): boolean {
    return this.inner.isClosed();
  }
}

function randomSessionResumeKey(): Uint8Array {
  const bytes = new Uint8Array(16);
  const cryptoApi = globalThis.crypto;
  if (!cryptoApi) {
    throw SessionError.protocol("crypto.getRandomValues is unavailable");
  }
  cryptoApi.getRandomValues(bytes);
  return bytes;
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
      {
        ...options,
        resumable: options.resumable ?? false,
      },
      established.recoverConduit,
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
    };
    const handshake = await handshakeAsInitiator(link, localSettings);
    return Session.initiatorConduit(new BareConduit(link), handshake, options);
  },

  async acceptorOnLink(
    link: Link,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    };
    const handshake = await handshakeAsAcceptor(link, localSettings);
    return Session.initiatorConduit(new BareConduit(link), handshake, options);
  },

  acceptorOrResume(
    conduit: Conduit<Message>,
    handshake: HandshakeResult,
    registry: SessionRegistry,
    options: SessionBuilderOptions = {},
  ): SessionAcceptOutcome {
    const resumeKey = handshake.peerResumeKey;
    if (resumeKey) {
      const handle = registry.get(resumeKey);
      if (!handle) {
        throw SessionError.protocol("unknown session resume key");
      }
      void handle.acceptResumedConduit(conduit);
      return { tag: "Resumed" };
    }
    const s = session.acceptorConduit(conduit, handshake, options);
    const establishedKey = handshake.sessionResumeKey;
    if (establishedKey) {
      registry.insert(establishedKey, s.handle());
    }
    return { tag: "Established", session: s };
  },

  async initiatorOn(
    transport: Link,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const established = await makeInitiatorEstablishedTransport(transport, options);
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      {
        ...options,
        resumable: false,
      },
      undefined,
    );
  },

  async acceptorOn(
    transport: Link,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const established = await makeAcceptorEstablishedTransport(transport, {
      ...options,
      resumable: options.resumable ?? false,
    });
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      {
        ...options,
        resumable: options.resumable ?? false,
      },
      undefined,
    );
  },

  async acceptorOnOrResume(
    transport: Link,
    registry: SessionRegistry,
    options: SessionTransportOptions = {},
  ): Promise<SessionAcceptOutcome> {
    const established = await makeAcceptorEstablishedTransport(transport, {
      ...options,
      resumable: options.resumable ?? false,
    });
    return session.acceptorOrResume(established.conduit, established.handshake, registry, options);
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
    const established = await makeAcceptorEstablishedTransport(transport, {
      ...options,
      resumable: options.resumable ?? false,
    });
    return Session.initiatorConduit(
      established.conduit,
      established.handshake,
      {
        ...options,
        resumable: options.resumable ?? false,
      },
      undefined,
    );
  },

  async acceptorTransportOrResume(
    transport: SessionTransport,
    registry: SessionRegistry,
    options: SessionTransportOptions = {},
  ): Promise<SessionAcceptOutcome> {
    const established = await makeAcceptorEstablishedTransport(transport, {
      ...options,
      resumable: options.resumable ?? false,
    });
    return session.acceptorOrResume(established.conduit, established.handshake, registry, options);
  },

  rootSettings(role: Role, maxConcurrentRequests = 64): ConnectionSettings {
    return {
      parity: parityFromRole(role),
      max_concurrent_requests: maxConcurrentRequests,
    };
  },
};
