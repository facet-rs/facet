import { describe, expect, it } from "vitest";
import { decodeTyped, encodeTyped } from "@bearcove/phon-engine";
import {
  type ConnectionSettings,
  decodeMessage,
  emptyMetadata,
  encodeMessage,
  type Message,
  messagePing,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageAccept,
  messageConnect,
  messageGoodbye,
  messageSchemaClosure,
  parseSchemaClosure,
  RpcError,
  RpcErrorCode,
} from "@bearcove/vox-wire";
import { Registry, hexToBytes, primitiveId, resolveIds } from "@bearcove/phon-schema";
import { BareConduit } from "./conduit.ts";
import { handshakeAsAcceptor, handshakeAsInitiator } from "./handshake.ts";
import {
  Session,
  ConnectionHandle,
  SessionError,
  session,
} from "./session.ts";
import { Role, channel, type MethodDescriptor } from "./channeling/index.ts";
import {
  sessionEchoRegistry,
  sessionEchoMethods,
  SESSION_ECHO_METHOD_ID,
} from "./session_echo.fixture.ts";
import { SchemaCompatibilityError, type PhonMethodSchemas } from "./schema_tracker.ts";
import {
  handshakeSchemaClosure,
  registry as handshakeRegistry,
  schemaId as handshakeSchemaId,
  type HandshakeMessage,
} from "./handshake.phon.generated.ts";

const ECHO_METHOD_KEY = `0x${SESSION_ECHO_METHOD_ID.toString(16).padStart(16, "0")}`;
const ECHO_METHOD_SCHEMAS = sessionEchoMethods[ECHO_METHOD_KEY]!;
const CHANNEL_ARGS_SCHEMAS = resolveIds([
  {
    id: 1n,
    typeParams: [],
    kind: {
      kind: "tuple",
      elements: [{ kind: "concrete", id: primitiveId("bytes"), args: [] }],
    },
  },
]);
const CHANNEL_METHOD: MethodDescriptor = {
  name: "sum",
  id: 77n,
};
const CHANNEL_METHOD_SCHEMAS: PhonMethodSchemas = {
  argsRoot: CHANNEL_ARGS_SCHEMAS[0]!.id,
  argsSchemaClosure: "",
  okRoot: primitiveId("u32"),
  responseRoot: primitiveId("u32"),
  responseSchemaClosure: "",
  channels: [{ index: 0, direction: "rx", elementRoot: primitiveId("u32") }],
};
const CHANNEL_REGISTRY = new Registry(CHANNEL_ARGS_SCHEMAS);

class MemoryLink {
  private readonly queue: Uint8Array[] = [];
  private waiting: ((value: Uint8Array | null) => void) | null = null;
  private closed = false;
  private readonly deliver: (payload: Uint8Array) => void;

  constructor(deliver: (payload: Uint8Array) => void) {
    this.deliver = deliver;
  }

  async send(payload: Uint8Array): Promise<void> {
    if (this.closed) {
      throw new Error("closed");
    }
    this.deliver(payload);
  }

  recv(): Promise<Uint8Array | null> {
    if (this.queue.length > 0) {
      return Promise.resolve(this.queue.shift()!);
    }
    if (this.closed) {
      return Promise.resolve(null);
    }
    return new Promise((resolve) => {
      this.waiting = resolve;
    });
  }

  push(payload: Uint8Array): void {
    if (this.closed) {
      return;
    }
    if (this.waiting) {
      const resolve = this.waiting;
      this.waiting = null;
      resolve(payload);
      return;
    }
    this.queue.push(payload);
  }

  close(): void {
    this.closed = true;
    const waiting = this.waiting;
    this.waiting = null;
    waiting?.(null);
  }

  isClosed(): boolean {
    return this.closed;
  }
}

function memoryLinkPair(): [MemoryLink, MemoryLink] {
  let left!: MemoryLink;
  let right!: MemoryLink;
  left = new MemoryLink((payload) => right.push(payload));
  right = new MemoryLink((payload) => left.push(payload));
  return [left, right];
}

function encodeHandshakeFrame(message: HandshakeMessage): Uint8Array {
  const value = encodeTyped(message as never, handshakeSchemaId.HandshakeMessage, handshakeRegistry);
  const closure = hexToBytes(handshakeSchemaClosure);
  const out = new Uint8Array(4 + closure.length + value.length);
  const dv = new DataView(out.buffer, out.byteOffset, out.byteLength);
  dv.setUint32(0, closure.length, true);
  out.set(closure, 4);
  out.set(value, 4 + closure.length);
  return out;
}

function decodeHandshakeFrame(bytes: Uint8Array): HandshakeMessage {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const len = dv.getUint32(0, true);
  const closure = bytes.subarray(4, 4 + len);
  const value = bytes.subarray(4 + len);
  const { root, schemas } = parseSchemaClosure(closure);
  return decodeTyped(
    value,
    root,
    handshakeSchemaId.HandshakeMessage,
    handshakeRegistry.with(schemas),
  ) as unknown as HandshakeMessage;
}

async function withTimeout<T>(
  promise: Promise<T>,
  label: string,
  timeoutMs = 1_000,
): Promise<T> {
  const timeout = new Promise<never>((_, reject) => {
    setTimeout(() => reject(new Error(`timed out waiting for ${label}`)), timeoutMs);
  });
  return Promise.race([promise, timeout]);
}

async function establishPair(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
): Promise<[Session, Session]> {
  const clientSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    max_concurrent_requests: 64,
    initial_channel_credit: 16,
  };
  const serverSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    max_concurrent_requests: 64,
    initial_channel_credit: 16,
  };
  const [clientHandshake, serverHandshake] = await Promise.all([
    handshakeAsInitiator(clientLink, clientSettings),
    handshakeAsAcceptor(serverLink, serverSettings),
  ]);
  const clientConduit = new BareConduit(clientLink);
  const serverConduit = new BareConduit(serverLink);
  const clientSession = session.initiatorConduit(clientConduit, clientHandshake);
  const serverSession = session.acceptorConduit(serverConduit, serverHandshake);
  return [clientSession, serverSession];
}

async function establishRawAcceptor(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
  options: Parameters<typeof session.acceptorConduit>[2] = {},
): Promise<Session> {
  const clientSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    max_concurrent_requests: 64,
    initial_channel_credit: 16,
  };
  const serverSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: 16,
  };
  const [, serverHandshake] = await Promise.all([
    handshakeAsInitiator(clientLink, clientSettings),
    handshakeAsAcceptor(serverLink, serverSettings),
  ]);
  return session.acceptorConduit(new BareConduit(serverLink), serverHandshake, options);
}

async function establishRawInitiator(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
  options: Parameters<typeof session.initiatorConduit>[2] = {},
): Promise<Session> {
  const clientSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    max_concurrent_requests: options.maxConcurrentRequests ?? 64,
    initial_channel_credit: 16,
  };
  const serverSettings: ConnectionSettings = {
    parity: { tag: "Even" },
    max_concurrent_requests: 64,
    initial_channel_credit: 16,
  };
  const [clientHandshake] = await Promise.all([
    handshakeAsInitiator(clientLink, clientSettings),
    handshakeAsAcceptor(serverLink, serverSettings),
  ]);
  return session.initiatorConduit(new BareConduit(clientLink), clientHandshake, options);
}

const ECHO_METHOD: MethodDescriptor = {
  name: "echo",
  id: SESSION_ECHO_METHOD_ID,
};

describe("session", () => {
  // r[verify session]
  // r[verify session.handshake]
  // r[verify session.handshake.phon]
  // r[verify session.handshake.protocol-schema]
  // r[verify session.handshake.protocol-schema.session-scoped]
  // r[verify session.handshake.unversioned]
  // r[verify session.connection-settings]
  // r[verify session.connection-settings.hello]
  // r[verify session.peer]
  // r[verify session.role]
  // r[verify session.symmetry]
  it("exchanges phon handshake schemas, settings, roles, and metadata", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const serverSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 8,
      initial_channel_credit: 4,
    };
    const acceptorSeedSettings: ConnectionSettings = {
      ...serverSettings,
      parity: { tag: "Odd" },
    };
    const clientMetadata = new Map([["client", "browser"]]);
    const serverMetadata = new Map([["server", "runtime"]]);

    const [clientHandshake, serverHandshake] = await Promise.all([
      handshakeAsInitiator(clientLink, clientSettings, clientMetadata),
      handshakeAsAcceptor(serverLink, acceptorSeedSettings, serverMetadata),
    ]);

    expect(clientHandshake.localSettings).toEqual(clientSettings);
    expect(clientHandshake.peerSettings).toEqual(serverSettings);
    expect(clientHandshake.peerMetadata.get("server")).toBe("runtime");
    expect(Array.from(clientHandshake.peerMessageSchema)).toEqual(
      Array.from(hexToBytes(messageSchemaClosure)),
    );

    expect(serverHandshake.localSettings).toEqual(serverSettings);
    expect(serverHandshake.peerSettings).toEqual(clientSettings);
    expect(serverHandshake.peerMetadata.get("client")).toBe("browser");
    expect(Array.from(serverHandshake.peerMessageSchema)).toEqual(
      Array.from(hexToBytes(messageSchemaClosure)),
    );

    clientLink.close();
    serverLink.close();
  });

  // r[verify session.handshake.sorry]
  // r[verify session.handshake.protocol-schema]
  it("acceptor sends Sorry when peer Message schema is invalid", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const serverSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const acceptor = handshakeAsAcceptor(serverLink, serverSettings)
      .then(() => undefined, (error: unknown) => error);

    await clientLink.send(
      encodeHandshakeFrame({
        tag: "Hello",
        value: {
          parity: { tag: "Odd" },
          connection_settings: {
            parity: { tag: "Odd" },
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
          },
          message_payload_schema: [0xFF, 0x00, 0xFF],
          metadata: emptyMetadata(),
        },
      }),
    );

    const response = decodeHandshakeFrame(
      (await withTimeout(clientLink.recv(), "acceptor handshake Sorry"))!,
    );
    expect(response.tag).toBe("Sorry");
    if (response.tag === "Sorry") {
      expect(response.value.reason).toBe("unsupported message compatibility plan");
    }
    const error = await withTimeout(acceptor, "acceptor rejection");
    expect(error).toBeInstanceOf(Error);
    expect(String(error)).toContain("unsupported message compatibility plan");

    clientLink.close();
    serverLink.close();
  });

  // r[verify session.handshake.sorry]
  // r[verify session.handshake.protocol-schema]
  it("initiator sends Sorry when peer Message schema is invalid", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const initiator = handshakeAsInitiator(clientLink, clientSettings)
      .then(() => undefined, (error: unknown) => error);

    const hello = decodeHandshakeFrame(
      (await withTimeout(serverLink.recv(), "initiator Hello"))!,
    );
    expect(hello.tag).toBe("Hello");

    await serverLink.send(
      encodeHandshakeFrame({
        tag: "HelloYourself",
        value: {
          connection_settings: {
            parity: { tag: "Even" },
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
          },
          message_payload_schema: [0xFF, 0x00, 0xFF],
          metadata: emptyMetadata(),
        },
      }),
    );

    const response = decodeHandshakeFrame(
      (await withTimeout(serverLink.recv(), "initiator handshake Sorry"))!,
    );
    expect(response.tag).toBe("Sorry");
    if (response.tag === "Sorry") {
      expect(response.value.reason).toBe("unsupported message compatibility plan");
    }
    const error = await withTimeout(initiator, "initiator rejection");
    expect(error).toBeInstanceOf(Error);
    expect(String(error)).toContain("unsupported message compatibility plan");

    clientLink.close();
    serverLink.close();
  });

  // r[verify session.parity]
  // r[verify connection.virtual]
  // r[verify connection.open]
  // r[verify rpc.virtual-connection.open]
  it("allocates virtual connection ids from local session parity", async () => {
    const requestedSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const acceptedSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };

    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink),
      "raw initiator establishment",
    );
    const initiatorFirst = initiatorSession.handle().openConnection(requestedSettings);
    const initiatorFirstOpen = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "initiator first connection open"))!,
    );
    expect(initiatorFirstOpen.connection_id).toBe(1n);
    expect(initiatorFirstOpen.payload.tag).toBe("ConnectionOpen");
    await rawServerLink.send(encodeMessage(messageAccept(1n, acceptedSettings)));
    await withTimeout(initiatorFirst, "initiator first connection accept");

    const initiatorSecond = initiatorSession.handle().openConnection(requestedSettings);
    const initiatorSecondOpen = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "initiator second connection open"))!,
    );
    expect(initiatorSecondOpen.connection_id).toBe(3n);
    expect(initiatorSecondOpen.payload.tag).toBe("ConnectionOpen");
    await rawServerLink.send(encodeMessage(messageAccept(3n, acceptedSettings)));
    await withTimeout(initiatorSecond, "initiator second connection accept");

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);

    const [rawClientLink, acceptorLink] = memoryLinkPair();
    const acceptorSession = await withTimeout(
      establishRawAcceptor(rawClientLink, acceptorLink),
      "raw acceptor establishment",
    );
    const acceptorFirst = acceptorSession.handle().openConnection(requestedSettings);
    const acceptorFirstOpen = decodeMessage(
      (await withTimeout(rawClientLink.recv(), "acceptor first connection open"))!,
    );
    expect(acceptorFirstOpen.connection_id).toBe(2n);
    expect(acceptorFirstOpen.payload.tag).toBe("ConnectionOpen");
    await rawClientLink.send(encodeMessage(messageAccept(2n, acceptedSettings)));
    await withTimeout(acceptorFirst, "acceptor first connection accept");

    const acceptorSecond = acceptorSession.handle().openConnection(requestedSettings);
    const acceptorSecondOpen = decodeMessage(
      (await withTimeout(rawClientLink.recv(), "acceptor second connection open"))!,
    );
    expect(acceptorSecondOpen.connection_id).toBe(4n);
    expect(acceptorSecondOpen.payload.tag).toBe("ConnectionOpen");
    await rawClientLink.send(encodeMessage(messageAccept(4n, acceptedSettings)));
    await withTimeout(acceptorSecond, "acceptor second connection accept");

    rawClientLink.close();
    acceptorLink.close();
    acceptorSession.handle().shutdown();
    await Promise.allSettled([acceptorSession.closed()]);
  });

  // r[verify connection.virtual]
  // r[verify rpc.virtual-connection.accept]
  // r[verify session.symmetry]
  // r[verify session.connection-settings.open]
  // r[verify session.message]
  // r[verify session.message.connection-id]
  // r[verify session.message.payloads]
  // r[verify rpc.request]
  // r[verify rpc.response]
  it("accepts inbound virtual connections and routes calls on them", async () => {
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [clientLink, rawServerLink] = memoryLinkPair();
    let acceptConnection!: (connection: ConnectionHandle) => void;
    const acceptedConnection = new Promise<ConnectionHandle>((resolve) => {
      acceptConnection = resolve;
    });
    const initiatorSession = await withTimeout(
      establishRawInitiator(clientLink, rawServerLink, {
        onConnection: (connection) => acceptConnection(connection),
      }),
      "raw initiator establishment",
    );

    await rawServerLink.send(
      encodeMessage(messageConnect(2n, peerSettings, emptyMetadata())),
    );
    const accept = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection accept"))!,
    );
    expect(accept.connection_id).toBe(2n);
    expect(accept.payload.tag).toBe("ConnectionAccept");
    const connection = await withTimeout(acceptedConnection, "accepted connection callback");
    expect(connection.id).toBe(2n);
    expect(connection.localSettings.parity).toEqual({ tag: "Even" });

    await rawServerLink.send(
      encodeMessage(
        messageRequest(
          77n,
          ECHO_METHOD.id,
          new Uint8Array([0x07]),
          emptyMetadata(),
          [],
          2n,
          Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.argsSchemaClosure)),
        ),
      ),
    );
    const incoming = await withTimeout(connection.nextIncomingCall(), "virtual incoming call");
    expect(incoming?.requestId).toBe(77n);
    expect(incoming?.methodId).toBe(ECHO_METHOD.id);
    expect(Array.from(incoming?.args ?? [])).toEqual([0x07]);

    await connection.sendResponse(77n, new Uint8Array([0x01]));
    const response = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection response"))!,
    );
    expect(response.connection_id).toBe(2n);
    expect(response.payload.tag).toBe("RequestMessage");
    if (response.payload.tag === "RequestMessage") {
      expect(response.payload.value.id).toBe(77n);
      expect(response.payload.value.body.tag).toBe("Response");
    }

    clientLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify connection.open.rejection]
  it("rejects inbound virtual connections when no acceptor is configured", async () => {
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [clientLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(clientLink, rawServerLink),
      "raw initiator establishment",
    );

    await rawServerLink.send(
      encodeMessage(messageConnect(2n, peerSettings, emptyMetadata())),
    );
    const reject = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection reject"))!,
    );
    expect(reject.connection_id).toBe(2n);
    expect(reject.payload.tag).toBe("ConnectionReject");

    clientLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify rpc.flow-control.credit.initial.high-level]
  // r[verify rpc.flow-control.credit.initial]
  // r[verify rpc.flow-control.credit.initial.zero]
  // r[verify rpc.flow-control.max-concurrent-requests.default]
  // r[verify connection.root]
  it("applies and rejects root channel capacity settings", () => {
    expect(session.rootSettings(Role.Acceptor)).toMatchObject({
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    });
    expect(session.rootSettings(Role.Initiator, 64, 7)).toMatchObject({
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 7,
    });
    expect(() => session.rootSettings(Role.Acceptor, 64, 0)).toThrow(/initial_channel_credit/);
  });

  // r[verify transport.prologue.first-payload]
  // r[verify transport.prologue.post-accept]
  // r[verify conduit]
  // r[verify conduit.bare]
  // r[verify conduit.typeplan]
  // r[verify connection]
  // r[verify connection.root]
  it("establishes over transport prologue before BareConduit traffic", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const [clientSession, serverSession] = await withTimeout(
      Promise.all([
        session.initiatorOn(clientLink),
        session.acceptorOn(serverLink),
      ]),
      "transport session establishment",
    );
    const serverRoot = serverSession.rootConnection();
    await expect(serverSession.handle().closeConnection(0n)).rejects.toThrow(
      /cannot close root connection/,
    );

    await clientLink.send(
      encodeMessage(
        messageRequest(1n, ECHO_METHOD.id, new Uint8Array(), emptyMetadata(), [], 0n, []),
      ),
    );

    await withTimeout(serverSession.closed(), "server protocol-error close");
    expect(serverRoot.isClosed()).toBe(true);

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed(), serverSession.closed()]);
  });

  // r[verify session.keepalive]
  it("answers keepalive pings with matching pongs", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const serverSession = await withTimeout(
      establishRawAcceptor(clientLink, serverLink),
      "raw acceptor establishment",
    );

    await clientLink.send(encodeMessage(messagePing(123n)));
    const payload = await withTimeout(clientLink.recv(), "keepalive pong");
    expect(payload).not.toBeNull();
    const message = decodeMessage(payload!);
    expect(message).toEqual({
      connection_id: 0n,
      payload: { tag: "Pong", value: { nonce: 123n } },
    });

    clientLink.close();
    serverLink.close();
    serverSession.handle().shutdown();
    await Promise.allSettled([serverSession.closed()]);
  });

  // r[verify session.keepalive]
  it("tears down when keepalive pong is missing", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const serverSession = await withTimeout(
      establishRawAcceptor(clientLink, serverLink, {
        keepaliveIntervalMs: 10,
        keepaliveTimeoutMs: 20,
      }),
      "raw keepalive acceptor establishment",
    );

    const payload = await withTimeout(clientLink.recv(), "keepalive ping");
    expect(payload).not.toBeNull();
    const message = decodeMessage(payload!);
    expect(message.payload).toEqual({ tag: "Ping", value: { nonce: 1n } });

    await withTimeout(serverSession.closed(), "keepalive timeout teardown");
    expect(serverLink.isClosed()).toBe(true);

    clientLink.close();
    serverLink.close();
  });

  // r[verify session.protocol-error]
  // r[verify rpc.observability.session-errors]
  it("sends protocol error before tearing down on local protocol violation", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const serverSession = await withTimeout(
      establishRawAcceptor(clientLink, serverLink),
      "raw protocol-error acceptor establishment",
    );

    await clientLink.send(
      encodeMessage(
        messageRequest(1n, ECHO_METHOD.id, new Uint8Array(), emptyMetadata(), [], 0n, []),
      ),
    );

    const payload = await withTimeout(clientLink.recv(), "protocol error frame");
    expect(payload).not.toBeNull();
    const message = decodeMessage(payload!);
    expect(message.connection_id).toBe(0n);
    expect(message.payload.tag).toBe("ProtocolError");
    if (message.payload.tag === "ProtocolError") {
      expect(message.payload.value.description).toContain("missing args schema binding");
    }

    await withTimeout(serverSession.closed(), "protocol-error teardown");

    clientLink.close();
    serverLink.close();
  });

  // r[verify schema.exchange.caller]
  it("advertises caller args schemas with the first request on a connection", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      0n,
      settings,
      settings,
    );

    const request = () =>
      connection.caller().call({
        method: "Test.echo",
        args: { value: 55 },
        descriptor: ECHO_METHOD,
        methodSchemas: ECHO_METHOD_SCHEMAS,
        registry: sessionEchoRegistry,
        timeoutMs: 1,
      });

    await Promise.allSettled([request(), request()]);

    const schemas = sent.map((message) => {
      expect(message.payload.tag).toBe("RequestMessage");
      const body = message.payload.tag === "RequestMessage"
        ? message.payload.value.body
        : undefined;
      expect(body?.tag).toBe("Call");
      return body?.tag === "Call" ? body.value.schemas : [];
    });

    expect(schemas).toEqual([
      Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.argsSchemaClosure)),
      [],
    ]);
  });

  // r[verify connection.parity]
  // r[verify session.message]
  // r[verify session.message.connection-id]
  // r[verify session.message.payloads]
  // r[verify rpc.request]
  // r[verify rpc.request.id-allocation]
  it("allocates request ids from virtual connection parity", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      13n,
      settings,
      settings,
    );

    const request = () =>
      connection.caller().call({
        method: "Test.echo",
        args: { value: 55 },
        descriptor: ECHO_METHOD,
        methodSchemas: ECHO_METHOD_SCHEMAS,
        registry: sessionEchoRegistry,
        timeoutMs: 1,
      });

    await Promise.allSettled([request(), request()]);

    const calls = sent.map((message) => {
      expect(message.connection_id).toBe(13n);
      expect(message.payload.tag).toBe("RequestMessage");
      const requestMessage = message.payload.tag === "RequestMessage"
        ? message.payload.value
        : undefined;
      expect(requestMessage?.body.tag).toBe("Call");
      return requestMessage?.id;
    });

    expect(calls).toEqual([2n, 4n]);
  });

  // r[verify rpc.flow-control.max-concurrent-requests]
  // r[verify rpc.flow-control.max-concurrent-requests.outbound]
  // r[verify rpc.flow-control.max-concurrent-requests.counting]
  // r[verify rpc.flow-control]
  // r[verify rpc.debug.snapshot]
  it("waits for peer request capacity before sending another call", async () => {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 1,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      0n,
      localSettings,
      peerSettings,
    );
    const request = () =>
      connection.caller().call({
        method: "Test.echo",
        args: { value: 55 },
        descriptor: ECHO_METHOD,
        methodSchemas: ECHO_METHOD_SCHEMAS,
        registry: sessionEchoRegistry,
        timeoutMs: 1_000,
      }).catch((error: unknown) => error);

    const first = request();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const second = request();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sent).toHaveLength(1);
    expect(connection.debugSnapshot()).toMatchObject({
      connectionId: 0n,
      pendingResponseCount: 1,
      inboundLiveRequestCount: 0,
      flowControl: {
        localMaxConcurrentRequests: 64,
        peerMaxConcurrentRequests: 1,
        outboundRequestLimit: {
          availablePermits: 0,
          waitingCount: 1,
          closed: false,
        },
      },
    });
    connection.resolveResponse(1n, Uint8Array.of(0));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sent).toHaveLength(2);
    const requestIds = sent.map((message) =>
      message.payload.tag === "RequestMessage" ? message.payload.value.id : null
    );
    expect(requestIds).toEqual([1n, 3n]);

    connection.close(SessionError.closed());
    await Promise.allSettled([first, second]);
  });

  // r[verify rpc.channel.binding.caller-args]
  // r[verify rpc.channel.binding.caller-args.rx]
  // r[verify rpc.channel.pair.binding-propagation]
  it("binds call channels before waiting for the first request-capacity turn", async () => {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 1,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      0n,
      localSettings,
      peerSettings,
    );
    const [tx, rx] = channel<number>();

    const call = connection.caller().call({
      method: "Test.sum",
      args: { numbers: rx },
      descriptor: CHANNEL_METHOD,
      methodSchemas: CHANNEL_METHOD_SCHEMAS,
      registry: CHANNEL_REGISTRY,
      timeoutMs: 1_000,
    });

    await tx.send(7);
    tx.close();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sent[0]?.payload.tag).toBe("RequestMessage");
    const request = sent[0]?.payload.tag === "RequestMessage"
      ? sent[0].payload.value.body
      : undefined;
    expect(request?.tag).toBe("Call");
    expect(request?.tag === "Call" ? request.value.channels : []).toHaveLength(1);
    expect(sent.slice(1).some((message) => message.payload.tag === "ChannelMessage")).toBe(true);

    connection.close(SessionError.closed());
    await expect(call).rejects.toBeInstanceOf(SessionError);
  });

  // r[verify rpc.cancel]
  // r[verify rpc.cancel.channels]
  it("queues inbound cancel without closing request channels", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const serverSession = await withTimeout(
      establishRawAcceptor(clientLink, serverLink),
      "raw acceptor establishment",
    );
    const serverRoot = serverSession.rootConnection();
    const schemas = Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.argsSchemaClosure));

    await clientLink.send(
      encodeMessage(
        messageRequest(
          1n,
          ECHO_METHOD.id,
          new Uint8Array(),
          emptyMetadata(),
          [11n],
          0n,
          schemas,
        ),
      ),
    );
    const incoming = await withTimeout(serverRoot.nextIncomingCall(), "incoming cancellable call");
    expect(incoming?.requestId).toBe(1n);
    expect(incoming?.channels).toEqual([11n]);

    const receiver = serverRoot.getChannelRegistry().registerIncoming(11n, 4);
    await clientLink.send(encodeMessage(messageCancel(1n)));
    expect(await withTimeout(serverRoot.nextIncomingCancel(), "incoming cancel")).toBe(1n);
    expect(serverRoot.debugSnapshot()).toMatchObject({
      closed: false,
      inboundLiveRequestCount: 0,
    });

    await clientLink.send(encodeMessage(messageData(11n, Uint8Array.of(4, 5, 6))));
    expect(Array.from((await withTimeout(receiver.recv(), "post-cancel channel item")) ?? []))
      .toEqual([4, 5, 6]);

    await serverRoot.sendResponse(1n, Uint8Array.of(9));
    const lateResponse = decodeMessage(
      (await withTimeout(clientLink.recv(), "post-cancel response"))!,
    );
    expect(lateResponse.connection_id).toBe(0n);
    expect(lateResponse.payload.tag).toBe("RequestMessage");
    if (lateResponse.payload.tag === "RequestMessage") {
      expect(lateResponse.payload.value.id).toBe(1n);
      expect(lateResponse.payload.value.body.tag).toBe("Response");
    }

    clientLink.close();
    serverLink.close();
    serverSession.handle().shutdown();
    await Promise.allSettled([serverSession.closed()]);
  });

  // r[verify rpc.flow-control.max-concurrent-requests.session-failure]
  it("fails calls waiting for request capacity when the connection closes", async () => {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 1,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      0n,
      localSettings,
      peerSettings,
    );
    const request = () =>
      connection.caller().call({
        method: "Test.echo",
        args: { value: 55 },
        descriptor: ECHO_METHOD,
        methodSchemas: ECHO_METHOD_SCHEMAS,
        registry: sessionEchoRegistry,
        timeoutMs: 1_000,
      });

    const first = request();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const second = request();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sent).toHaveLength(1);
    connection.close(SessionError.closed());

    await expect(first).rejects.toBeInstanceOf(SessionError);
    await expect(second).rejects.toBeInstanceOf(SessionError);
    expect(sent).toHaveLength(1);
  });

  // r[verify rpc.flow-control.max-concurrent-requests]
  // r[verify rpc.flow-control.max-concurrent-requests.inbound]
  it("treats inbound request attempts beyond the local limit as protocol errors", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const serverSession = await withTimeout(
      establishRawAcceptor(clientLink, serverLink, { maxConcurrentRequests: 1 }),
      "raw limited acceptor establishment",
    );
    const schemas = Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.argsSchemaClosure));

    await clientLink.send(
      encodeMessage(
        messageRequest(
          1n,
          ECHO_METHOD.id,
          new Uint8Array(),
          emptyMetadata(),
          [],
          0n,
          schemas,
        ),
      ),
    );
    await clientLink.send(
      encodeMessage(
        messageRequest(
          3n,
          ECHO_METHOD.id,
          new Uint8Array(),
          emptyMetadata(),
          [],
          0n,
          schemas,
        ),
      ),
    );

    const payload = await withTimeout(clientLink.recv(), "max-concurrent protocol error");
    expect(payload).not.toBeNull();
    const message = decodeMessage(payload!);
    expect(message.connection_id).toBe(0n);
    expect(message.payload.tag).toBe("ProtocolError");
    if (message.payload.tag === "ProtocolError") {
      expect(message.payload.value.description).toContain("max_concurrent_requests");
    }
    await withTimeout(serverSession.closed(), "max-concurrent teardown");

    clientLink.close();
    serverLink.close();
  });

  // r[verify connection.close]
  // r[verify rpc.caller.liveness.last-drop-closes-connection]
  it("closes a virtual connection when its last caller is disposed", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink),
      "raw initiator establishment",
    );

    const opened = initiatorSession.handle().openConnection(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection open"))!,
    );
    expect(open.connection_id).toBe(1n);
    await rawServerLink.send(encodeMessage(messageAccept(1n, peerSettings)));
    const connection = await withTimeout(opened, "virtual connection accept");

    const caller = connection.caller();
    caller.dispose();

    const close = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection close"))!,
    );
    expect(close.connection_id).toBe(1n);
    expect(close.payload.tag).toBe("ConnectionClose");

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify rpc.caller.liveness.refcounted]
  it("keeps a virtual connection live until all callers are disposed", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink),
      "raw initiator establishment",
    );

    const opened = initiatorSession.handle().openConnection(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection open"))!,
    );
    await rawServerLink.send(encodeMessage(messageAccept(open.connection_id, peerSettings)));
    const connection = await withTimeout(opened, "virtual connection accept");

    const firstCaller = connection.caller();
    const secondCaller = connection.caller();

    firstCaller.dispose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(connection.isClosed()).toBe(false);

    secondCaller.dispose();
    const close = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection close after last caller"))!,
    );
    expect(close.connection_id).toBe(open.connection_id);
    expect(close.payload.tag).toBe("ConnectionClose");
    expect(connection.isClosed()).toBe(true);

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify connection.close.semantics]
  it("tears down a virtual connection after receiving close", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink),
      "raw initiator establishment",
    );

    const opened = initiatorSession.handle().openConnection(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection open"))!,
    );
    await rawServerLink.send(encodeMessage(messageAccept(open.connection_id, peerSettings)));
    const connection = await withTimeout(opened, "virtual connection accept");
    expect(connection.isClosed()).toBe(false);

    await rawServerLink.send(encodeMessage(messageGoodbye(open.connection_id)));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(connection.isClosed()).toBe(true);

    await rawServerLink.send(
      encodeMessage(
        messageRequest(
          7n,
          ECHO_METHOD.id,
          new Uint8Array(),
          emptyMetadata(),
          [],
          open.connection_id,
          [],
        ),
      ),
    );
    const errorPayload = await withTimeout(rawServerLink.recv(), "post-close protocol error");
    expect(errorPayload).not.toBeNull();
    const errorMessage = decodeMessage(errorPayload!);
    expect(errorMessage.connection_id).toBe(0n);
    expect(errorMessage.payload.tag).toBe("ProtocolError");
    if (errorMessage.payload.tag === "ProtocolError") {
      expect(errorMessage.payload.value.description).toContain(
        `unknown connection ${open.connection_id}`,
      );
    }

    await withTimeout(initiatorSession.closed(), "post-close protocol teardown");
    rawServerLink.close();
  });

  // r[verify rpc.caller.liveness.root-internal-close]
  // r[verify rpc.caller.liveness.root-teardown-condition]
  it("tears down after root caller disposal only once virtual callers are gone", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink),
      "raw initiator establishment",
    );

    const opened = initiatorSession.handle().openConnection(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual connection open"))!,
    );
    await rawServerLink.send(encodeMessage(messageAccept(open.connection_id, peerSettings)));
    const connection = await withTimeout(opened, "virtual connection accept");

    const rootCaller = initiatorSession.rootConnection().caller();
    const virtualCaller = connection.caller();

    rootCaller.dispose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(initiatorLink.isClosed()).toBe(false);

    virtualCaller.dispose();
    const close = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "virtual close after root disposal"))!,
    );
    expect(close.connection_id).toBe(open.connection_id);
    expect(close.payload.tag).toBe("ConnectionClose");

    await withTimeout(initiatorSession.closed(), "root liveness teardown");
    expect(initiatorLink.isClosed()).toBe(true);

    rawServerLink.close();
  });

  // r[verify schema.exchange.required]
  it("tears down when a call arrives without an args schema binding", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const [clientSession, serverSession] = await withTimeout(
      establishPair(clientLink, serverLink),
      "session establishment",
    );
    const serverRoot = serverSession.rootConnection();

    await clientLink.send(
      encodeMessage(
        messageRequest(1n, ECHO_METHOD.id, new Uint8Array(), emptyMetadata(), [], 0n, []),
      ),
    );

    await withTimeout(serverSession.closed(), "server protocol-error close");
    expect(serverRoot.isClosed()).toBe(true);

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed(), serverSession.closed()]);
  });

  // r[verify schema.exchange.required]
  it("tears down when a response arrives without a response schema binding", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const [clientSession, serverSession] = await withTimeout(
      establishPair(clientLink, serverLink),
      "session establishment",
    );
    const clientRoot = clientSession.rootConnection();

    const call = clientRoot.caller().call({
      method: "Test.echo",
      args: { value: 55 },
      descriptor: ECHO_METHOD,
      methodSchemas: ECHO_METHOD_SCHEMAS,
      registry: sessionEchoRegistry,
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    await serverLink.send(
      encodeMessage(messageResponse(1n, new Uint8Array(), emptyMetadata(), 0n, [])),
    );

    await withTimeout(clientSession.closed(), "client protocol-error close");
    await expect(call).rejects.toBeInstanceOf(SessionError);
    expect(clientRoot.isClosed()).toBe(true);

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed(), serverSession.closed()]);
  });

  // r[verify schema.errors.call-level]
  // r[verify schema.errors.call-level.caller]
  // r[verify schema.errors.same-peer-terminal]
  it("rejects only the call when caller response decode fails", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      0n,
      settings,
      settings,
    );
    const tracker = connection.getSchemaTracker();
    tracker.recordReceived(
      ECHO_METHOD.id,
      "response",
      hexToBytes(ECHO_METHOD_SCHEMAS.responseSchemaClosure),
    );
    tracker.buildWriterDecoder = () => () => {
      throw new SchemaCompatibilityError("response decode plan failed");
    };

    const call = connection.caller().call({
      method: "Test.echo",
      args: { value: 55 },
      descriptor: ECHO_METHOD,
      methodSchemas: ECHO_METHOD_SCHEMAS,
      registry: sessionEchoRegistry,
      timeoutMs: 1_000,
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(sent).toHaveLength(1);
    connection.resolveResponse(1n, Uint8Array.of(0));

    await expect(call).rejects.toBeInstanceOf(SchemaCompatibilityError);
    expect(connection.isClosed()).toBe(false);
    expect(sent).toHaveLength(1);
  });

  // r[verify rpc.error.scope]
  // r[verify rpc.fallible]
  // r[verify rpc.fallible.vox-error]
  // r[verify rpc.fallible.vox-error.outcome]
  it("maps VoxError response variants to RpcError without closing the connection", async () => {
    const cases = [
      {
        wireError: { tag: "User", value: {} },
        code: RpcErrorCode.USER,
        user: true,
      },
      {
        wireError: { tag: "UnknownMethod" },
        code: RpcErrorCode.UNKNOWN_METHOD,
        user: false,
      },
      {
        wireError: { tag: "InvalidPayload", value: "bad args" },
        code: RpcErrorCode.INVALID_PAYLOAD,
        user: false,
      },
      {
        wireError: { tag: "Cancelled" },
        code: RpcErrorCode.CANCELLED,
        user: false,
      },
      {
        wireError: { tag: "ConnectionClosed" },
        code: RpcErrorCode.INDETERMINATE,
        user: false,
      },
      {
        wireError: { tag: "SessionShutdown" },
        code: RpcErrorCode.INDETERMINATE,
        user: false,
      },
      {
        wireError: { tag: "SendFailed" },
        code: RpcErrorCode.INDETERMINATE,
        user: false,
      },
      {
        wireError: { tag: "Indeterminate" },
        code: RpcErrorCode.INDETERMINATE,
        user: false,
      },
    ] satisfies {
      wireError: { tag: string; value?: unknown };
      code: RpcErrorCode;
      user: boolean;
    }[];

    for (const { wireError, code, user } of cases) {
      const settings: ConnectionSettings = {
        parity: { tag: "Odd" },
        max_concurrent_requests: 64,
        initial_channel_credit: 16,
      };
      const sent: Message[] = [];
      const fakeSession = {
        sendMessage: async (message: Message) => {
          sent.push(message);
        },
      };
      const connection = new ConnectionHandle(
        fakeSession as never,
        0n,
        settings,
        settings,
      );
      connection.getSchemaTracker().recordReceived(
        ECHO_METHOD.id,
        "response",
        hexToBytes(ECHO_METHOD_SCHEMAS.responseSchemaClosure),
      );

      const call = connection.caller().call({
        method: "Test.echo",
        args: { value: 55 },
        descriptor: ECHO_METHOD,
        methodSchemas: ECHO_METHOD_SCHEMAS,
        registry: sessionEchoRegistry,
        timeoutMs: 1_000,
      });
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(sent).toHaveLength(1);

      const payload = encodeTyped(
        { tag: "Err", value: wireError } as never,
        ECHO_METHOD_SCHEMAS.responseRoot,
        sessionEchoRegistry,
      );
      connection.resolveResponse(1n, payload);

      let thrown: unknown = null;
      try {
        await call;
      } catch (error) {
        thrown = error;
      }

      expect(thrown).toBeInstanceOf(RpcError);
      const rpcError = thrown as RpcError;
      expect(rpcError.code).toBe(code);
      expect(rpcError.isUserError()).toBe(user);
      expect(rpcError.isProtocolError()).toBe(!user);
      if (user) {
        expect(rpcError.userError).toEqual({});
      }
      expect(connection.isClosed()).toBe(false);
    }
  });

  it("restarts channel flushing when new work arrives during a pending exit", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const sent: Message[] = [];
    const fakeSession = {
      sendMessage: async (message: Message) => {
        sent.push(message);
      },
    };
    const connection = new ConnectionHandle(
      fakeSession as never,
      0n,
      settings,
      settings,
    );

    let pollCount = 0;
    const fakeRegistry = {
      pollOutgoing() {
        pollCount += 1;
        if (pollCount === 1) {
          void connection.flushOutgoing();
          return { kind: "pending" } as const;
        }
        if (pollCount === 2) {
          return {
            kind: "data",
            channelId: 7n,
            payload: Uint8Array.of(1, 2, 3),
          } as const;
        }
        return { kind: "done" } as const;
      },
    };
    (
      connection as unknown as {
        channelRegistry: typeof fakeRegistry;
      }
    ).channelRegistry = fakeRegistry;

    await connection.flushOutgoing();

    expect(pollCount).toBe(3);
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({
      connection_id: 0n,
      payload: {
        tag: "ChannelMessage",
        value: {
          id: 7n,
          body: {
            tag: "Item",
          },
        },
      },
    });
    expect(
      Array.from(
        sent[0].payload.tag === "ChannelMessage"
          ? sent[0].payload.value.body.tag === "Item"
            ? sent[0].payload.value.body.value.item
            : new Uint8Array(0)
          : new Uint8Array(0),
      ),
    ).toEqual([1, 2, 3]);
  });
});
