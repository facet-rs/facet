import { decodeWithSchema, encodeWithSchema } from "@bearcove/roam-postcard";
import {
  type ConnectionSettings,
  type RequestMessage,
  type ChannelMessage,
  type Hello,
  type HelloYourself,
  type Message,
  type Metadata,
  type MetadataEntry,
  helloV7,
  messageAccept,
  messageConnect,
  messageGoodbye,
  messageHello,
  messageHelloYourself,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageCredit,
  parityEven,
  parityOdd,
  RpcError,
  RpcErrorCode,
} from "@bearcove/roam-wire";
import {
  ChannelError,
  ChannelIdAllocator,
  ChannelRegistry,
  Role,
  type MethodDescriptor,
  type SchemaRegistry,
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
import type { Link, LinkSource } from "./link.ts";
import { singleLinkSource } from "./link.ts";
import { StableConduit } from "./stable_conduit.ts";

const DEFAULT_TIMEOUT_MS = 30_000;

interface PendingResponse {
  resolve: (payload: Uint8Array) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
  methodId: bigint;
  payload: Uint8Array;
  metadata: Metadata;
  channels: bigint[];
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

export interface SessionBuilderOptions {
  maxConcurrentRequests?: number;
  metadata?: Metadata;
  onConnection?: (connection: ConnectionHandle) => void | Promise<void>;
  resumable?: boolean;
}

export type SessionConduitKind = "bare" | "stable";

export interface SessionTransportOptions extends SessionBuilderOptions {
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

async function makeSessionConduit(
  transport: SessionTransport,
  options: SessionTransportOptions,
): Promise<Conduit<Message>> {
  // r[impl conduit.bare]
  // r[impl conduit.stable]
  const conduit = options.conduit ?? "bare";
  if (isLinkSource(transport)) {
    if (conduit === "stable") {
      return StableConduit.connect(transport);
    }

    const attachment = await transport.nextLink();
    return new BareConduit(attachment.link);
  }

  if (conduit === "stable") {
    return StableConduit.connect(singleLinkSource(transport));
  }

  return new BareConduit(transport);
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
    private readonly onConnection?: (connection: ConnectionHandle) => void | Promise<void>,
  ) {
    this.conduit = conduit;
    this.peerSupportsRetry = peerSupportsRetry;
    this.resumable = resumable;
    this.sessionResumeKey = sessionResumeKey?.slice() ?? null;
    this.nextConnectionId = firstIdForParity(localRootSettings.parity);
    this.sessionHandle = new SessionHandle(this);
  }

  sessionHandleValue(): SessionHandle {
    return this.sessionHandle;
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
      this.fail(error instanceof SessionError ? error : new SessionError(String(error)));
    });
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
        if (await this.handleConduitBreak()) {
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

  private async handleConduitBreak(): Promise<boolean> {
    if (!this.resumable) {
      return false;
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
    const resumeKey = this.sessionResumeKey;
    if (!resumeKey) {
      throw SessionError.protocol("session is not resumable");
    }

    if (this.localRootSettings.parity.tag === "Odd") {
      const helloMetadata = appendSessionResumeKeyMetadata(
        appendRetrySupportMetadata([]),
        resumeKey,
      );
      await conduit.send(
        messageHello(
          helloV7(
            this.localRootSettings.parity,
            this.localRootSettings.max_concurrent_requests,
            helloMetadata,
          ),
        ),
      );
      const helloYourself = await waitForHelloYourself(conduit);
      if (
        helloYourself.connection_settings.parity.tag !== this.peerRootSettings.parity.tag
        || helloYourself.connection_settings.max_concurrent_requests
          !== this.peerRootSettings.max_concurrent_requests
      ) {
        throw SessionError.protocol(
          `peer root settings changed across session resume: expected ${JSON.stringify(this.peerRootSettings)}, got ${JSON.stringify(helloYourself.connection_settings)}`,
        );
      }
      const echoedKey = metadataSessionResumeKey(helloYourself.metadata);
      if (!echoedKey || !sameBytes(echoedKey, resumeKey)) {
        throw SessionError.protocol("session resume key mismatch");
      }
      this.peerSupportsRetry = metadataSupportsRetry(helloYourself.metadata);
      this.conduit = conduit;
      return;
    }

    const hello = await waitForHello(conduit);
    if (
      hello.connection_settings.parity.tag !== this.peerRootSettings.parity.tag
      || hello.connection_settings.max_concurrent_requests
        !== this.peerRootSettings.max_concurrent_requests
    ) {
      throw SessionError.protocol(
        `peer root settings changed across session resume: expected ${JSON.stringify(this.peerRootSettings)}, got ${JSON.stringify(hello.connection_settings)}`,
      );
    }
    const actualKey = metadataSessionResumeKey(hello.metadata);
    if (!actualKey || !sameBytes(actualKey, resumeKey)) {
      throw SessionError.protocol("session resume key mismatch");
    }

    const helloMetadata = appendSessionResumeKeyMetadata(
      appendRetrySupportMetadata([]),
      resumeKey,
    );
    await conduit.send(
      messageHelloYourself({
        connection_settings: this.localRootSettings,
        metadata: helloMetadata,
      }),
    );
    this.peerSupportsRetry = metadataSupportsRetry(hello.metadata);
    this.conduit = conduit;
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
    switch (message.payload.tag) {
      case "Hello":
      case "HelloYourself":
        return;

      case "Ping":
      case "Pong":
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
      case "Call":
        connection.enqueueIncomingCall({
          requestId: request.id,
          methodId: request.body.value.method_id,
          args: request.body.value.args,
          channels: request.body.value.channels,
          metadata: request.body.value.metadata,
        });
        return;

      case "Response":
        connection.resolveResponse(request.id, request.body.value.ret);
        return;

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

  resume(conduit: Conduit<Message>): Promise<void> {
    return this.core.resume(conduit);
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

  isClosed(): boolean {
    return this.closed;
  }

  async call(request: CallerRequest): Promise<unknown> {
    if (this.closed) {
      throw SessionError.closed();
    }

    const values = Object.values(request.args);
    const payload =
      values.length === 0
        ? new Uint8Array(0)
        : encodeWithSchema(values, request.descriptor.args, request.schemaRegistry);
    const metadataCarrier = request.metadata ? request.metadata.clone() : new ClientMetadata();
    if (this.peerSupportsRetry) {
      ensureOperationId(metadataCarrier, this.nextOperationId++);
    }
    const metadata = clientMetadataToEntries(metadataCarrier);
    const channels = request.channels ?? [];
    const requestId = this.nextRequestId;
    this.nextRequestId += 2n;

    const responsePayload = await new Promise<Uint8Array>((resolve, reject) => {
      const state: PendingResponse = {
        resolve,
        reject,
        timer: setTimeout(() => {
          this.clearPendingState(state);
          reject(new SessionError("timeout waiting for response"));
        }, request.timeoutMs ?? DEFAULT_TIMEOUT_MS),
        methodId: request.descriptor.id,
        payload: payload.slice(),
        metadata: cloneMetadata(metadata),
        channels: [...channels],
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
  ): Promise<void> {
    await this.session.sendMessage(messageResponse(requestId, payload, metadata, channels, this.id));
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
      pending.reject(error);
    }
  }

  onSessionResumed(): void {
    if (this.closed || !this.peerSupportsRetry) {
      return;
    }
    const states = new Set(this.pendingResponses.values());
    for (const state of states) {
      if (state.settled) {
        continue;
      }
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
      await this.session.sendMessage(
        messageRequest(
          requestId,
          state.methodId,
          state.payload,
          cloneMetadata(state.metadata),
          [...state.channels],
          this.id,
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

  static async establishInitiator(
    conduit: Conduit<Message>,
    options: SessionBuilderOptions = {},
  ): Promise<Session> {
    // r[impl session.handshake]
    // r[impl session.connection-settings.hello]
    // r[impl session.parity]
    const localSettings: ConnectionSettings = {
      parity: parityOdd(),
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    };
    const helloMetadata = appendRetrySupportMetadata(options.metadata ?? []);
    await conduit.send(
      messageHello(helloV7(localSettings.parity, localSettings.max_concurrent_requests, helloMetadata)),
    );
    const helloYourself = await waitForHelloYourself(conduit);
    const sessionResumeKey = metadataSessionResumeKey(helloYourself.metadata);
    if (options.resumable && !sessionResumeKey) {
      throw SessionError.protocol("peer did not advertise session resumption");
    }
    const core = new SessionCore(
      conduit,
      localSettings,
      helloYourself.connection_settings,
      metadataSupportsRetry(helloYourself.metadata),
      options.resumable ?? false,
      sessionResumeKey,
      options.onConnection,
    );
    core.rootConnection();
    core.start();
    return new Session(core);
  }

  static async establishAcceptor(
    conduit: Conduit<Message>,
    options: SessionBuilderOptions = {},
  ): Promise<Session> {
    // r[impl session.handshake]
    // r[impl session.connection-settings.hello]
    // r[impl session.parity]
    const hello = await waitForHello(conduit);
    const localSettings: ConnectionSettings = {
      parity: parityEven(),
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    };
    let helloMetadata = appendRetrySupportMetadata(options.metadata ?? []);
    let sessionResumeKey: Uint8Array | null = null;
    if (options.resumable) {
      sessionResumeKey = randomSessionResumeKey();
      helloMetadata = appendSessionResumeKeyMetadata(helloMetadata, sessionResumeKey);
    }
    const response: HelloYourself = {
      connection_settings: localSettings,
      metadata: helloMetadata,
    };
    await conduit.send(messageHelloYourself(response));
    const core = new SessionCore(
      conduit,
      localSettings,
      hello.connection_settings,
      metadataSupportsRetry(hello.metadata),
      options.resumable ?? false,
      sessionResumeKey,
      options.onConnection,
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

async function waitForHello(conduit: Conduit<Message>): Promise<Hello> {
  const message = await conduit.recv();
  if (!message) {
    throw SessionError.closed();
  }
  if (message.payload.tag !== "Hello") {
    throw SessionError.protocol("expected Hello during session establishment");
  }
  return message.payload.value;
}

async function waitForHelloYourself(conduit: Conduit<Message>): Promise<HelloYourself> {
  const message = await conduit.recv();
  if (!message) {
    throw SessionError.closed();
  }
  if (message.payload.tag !== "HelloYourself") {
    throw SessionError.protocol("expected HelloYourself during session establishment");
  }
  return message.payload.value;
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
  initiator(conduit: Conduit<Message>, options: SessionBuilderOptions = {}): Promise<Session> {
    return Session.establishInitiator(conduit, options);
  },

  acceptor(conduit: Conduit<Message>, options: SessionBuilderOptions = {}): Promise<Session> {
    return Session.establishAcceptor(conduit, options);
  },

  async initiatorTransport(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const conduit = await makeSessionConduit(transport, options);
    return Session.establishInitiator(conduit, options);
  },

  async acceptorTransport(
    transport: SessionTransport,
    options: SessionTransportOptions = {},
  ): Promise<Session> {
    const conduit = await makeSessionConduit(transport, options);
    return Session.establishAcceptor(conduit, options);
  },

  rootSettings(role: Role, maxConcurrentRequests = 64): ConnectionSettings {
    return {
      parity: parityFromRole(role),
      max_concurrent_requests: maxConcurrentRequests,
    };
  },
};
