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
import { clientMetadataToEntries } from "./metadata.ts";
import type { Conduit } from "./conduit.ts";
import { AsyncQueue } from "./internal/async_queue.ts";
import { deferred, type Deferred } from "./internal/deferred.ts";
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
}

export type SessionConduitKind = "bare" | "stable";

export interface SessionTransportOptions extends SessionBuilderOptions {
  conduit?: SessionConduitKind;
}

type SessionTransport = Link | LinkSource;

function isLinkSource(value: SessionTransport): value is LinkSource {
  return typeof (value as LinkSource).nextLink === "function";
}

async function makeSessionConduit(
  transport: SessionTransport,
  options: SessionTransportOptions,
): Promise<Conduit<Message>> {
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

  constructor(
    private readonly conduit: Conduit<Message>,
    private readonly localRootSettings: ConnectionSettings,
    private readonly peerRootSettings: ConnectionSettings,
    private readonly onConnection?: (connection: ConnectionHandle) => void | Promise<void>,
  ) {
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
      );
      this.connections.set(0n, this.rootConnectionValue);
    }
    return this.rootConnectionValue;
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
    this.assertOpen();
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
    this.assertOpen();

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

    for (const pending of this.pendingConnections.values()) {
      pending.result.reject(error);
    }
    this.pendingConnections.clear();

    for (const connection of this.connections.values()) {
      connection.close(error);
    }
    this.connections.clear();
  }

  private assertOpen(): void {
    if (this.closed) {
      throw this.closeError ?? SessionError.closed();
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
    while (!this.closed) {
      const message = await this.conduit.recv();
      if (!message) {
        throw SessionError.closed();
      }
      await this.handleMessage(message);
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
        return;
    }
  }

  private handleChannelMessage(
    connectionId: bigint,
    channel: ChannelMessage,
  ): void {
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

  closed(): Promise<void> {
    return this.core.closedPromise();
  }
}

export class ConnectionHandle {
  private readonly role: Role;
  private readonly channelAllocator: ChannelIdAllocator;
  private readonly channelRegistry: ChannelRegistry;
  private readonly incomingCalls = new AsyncQueue<IncomingCall>();
  private readonly pendingResponses = new Map<bigint, PendingResponse>();
  private nextRequestId: bigint;
  private closed = false;
  private flushPromise: Promise<void> | null = null;

  constructor(
    private readonly session: SessionCore,
    readonly id: bigint,
    readonly localSettings: ConnectionSettings,
    readonly peerSettings: ConnectionSettings,
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
    const metadata = request.metadata ? clientMetadataToEntries(request.metadata) : [];
    const channels = request.channels ?? [];
    const requestId = this.nextRequestId;
    this.nextRequestId += 2n;

    const responsePayload = await new Promise<Uint8Array>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingResponses.delete(requestId);
        reject(new SessionError("timeout waiting for response"));
      }, request.timeoutMs ?? DEFAULT_TIMEOUT_MS);
      this.pendingResponses.set(requestId, { resolve, reject, timer });

      void this.session
        .sendMessage(
          messageRequest(
            requestId,
            request.descriptor.id,
            payload,
            metadata,
            channels,
            this.id,
          ),
        )
        .then(() => this.flushOutgoing())
        .catch((error) => {
          clearTimeout(timer);
          this.pendingResponses.delete(requestId);
          reject(error instanceof Error ? error : new SessionError(String(error)));
        });
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

  resolveResponse(requestId: bigint, payload: Uint8Array): void {
    const pending = this.pendingResponses.get(requestId);
    if (!pending) {
      return;
    }
    clearTimeout(pending.timer);
    this.pendingResponses.delete(requestId);
    pending.resolve(payload);
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
    this.channelRegistry.closeAll();
    for (const [requestId, pending] of this.pendingResponses) {
      clearTimeout(pending.timer);
      pending.reject(error);
      this.pendingResponses.delete(requestId);
    }
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
    const localSettings: ConnectionSettings = {
      parity: parityOdd(),
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    };
    await conduit.send(messageHello(helloV7(localSettings.parity, localSettings.max_concurrent_requests, options.metadata ?? [])));
    const helloYourself = await waitForHelloYourself(conduit);
    const core = new SessionCore(
      conduit,
      localSettings,
      helloYourself.connection_settings,
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
    const hello = await waitForHello(conduit);
    const localSettings: ConnectionSettings = {
      parity: parityEven(),
      max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    };
    const response: HelloYourself = {
      connection_settings: localSettings,
      metadata: options.metadata ?? [],
    };
    await conduit.send(messageHelloYourself(response));
    const core = new SessionCore(
      conduit,
      localSettings,
      hello.connection_settings,
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
