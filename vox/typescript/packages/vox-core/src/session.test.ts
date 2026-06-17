import { describe, expect, it, vi } from "vitest";
import { decodeTyped, encodeTyped } from "@bearcove/phon-engine";
import {
  type ConnectionSettings,
  coerceMetadata,
  decodeMessage,
  emptyMetadata,
  encodeMessage,
  type Message,
  messagePing,
  messageRequest,
  messageResponse,
  messageCancel,
  messageLaneAccept,
  messageLaneOpen,
  messageLaneReject,
  messageLaneClose,
  messageSchemaClosure,
  parseSchemaClosure,
  RpcError,
  RpcErrorCode,
} from "@bearcove/vox-wire";
import { Registry, hexToBytes, primitiveId, resolveIds } from "@bearcove/phon-schema";
import { BareConduit } from "./conduit.ts";
import { handshakeAsAcceptor, handshakeAsInitiator } from "./handshake.ts";
import {
  Connection,
  Lane,
  ConnectionError,
  LaneRejection,
  accept,
  acceptOnLink,
  connect,
  connectOnLink,
  defaultLaneSettings,
} from "./session.ts";
import type { EstablishmentEvent } from "./observer.ts";
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

  queuedPayloadCount(): number {
    return this.queue.length;
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

function establishmentObserver(events: EstablishmentEvent[]): {
  establishment: (event: EstablishmentEvent) => void;
} {
  return {
    establishment: (event) => events.push(event),
  };
}

function establishmentLabels(events: EstablishmentEvent[]): string[] {
  return events.map((event) => ({
    kind: event.kind,
    role: event.context.role,
    phase: event.context.phase,
    laneId: event.context.laneId?.toString() ?? "-",
    outcome: event.kind === "finished" ? event.outcome : "-",
  })).map(({ kind, role, phase, laneId, outcome }) =>
    `${kind}:${role}:${phase}:${laneId}:${outcome}`
  );
}

async function establishPair(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
): Promise<[Connection, Connection]> {
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
  const clientSession = Connection.connectConduit(clientConduit, clientHandshake);
  const serverSession = Connection.acceptConduit(serverConduit, serverHandshake);
  return [clientSession, serverSession];
}

async function establishRawAcceptor(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
  options: Parameters<typeof Connection.acceptConduit>[2] = {},
): Promise<Connection> {
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
  return Connection.acceptConduit(new BareConduit(serverLink), serverHandshake, options);
}

async function establishRawInitiator(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
  options: Parameters<typeof Connection.connectConduit>[2] = {},
): Promise<Connection> {
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
  return Connection.connectConduit(new BareConduit(clientLink), clientHandshake, options);
}

async function establishInboundServiceLane(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
): Promise<{ serverSession: Connection; serverLane: Lane; laneId: bigint }> {
  const laneId = 1n;
  const peerSettings: ConnectionSettings = {
    parity: { tag: "Odd" },
    max_concurrent_requests: 64,
    initial_channel_credit: 16,
  };
  let acceptLane!: (lane: Lane) => void;
  const acceptedLane = new Promise<Lane>((resolve) => {
    acceptLane = resolve;
  });
  const serverSession = await withTimeout(
    establishRawAcceptor(clientLink, serverLink, {
      onLane: (lane) => acceptLane(lane),
    }),
    "raw acceptor establishment",
  );

  await clientLink.send(encodeMessage(messageLaneOpen(laneId, peerSettings, emptyMetadata())));
  const accept = decodeMessage(
    (await withTimeout(clientLink.recv(), "service lane accept"))!,
  );
  expect(accept.lane_id).toBe(laneId);
  expect(accept.payload.tag).toBe("LaneAccept");
  const serverLane = await withTimeout(acceptedLane, "accepted service lane");
  expect(serverLane.id).toBe(laneId);

  return { serverSession, serverLane, laneId };
}

const ECHO_METHOD: MethodDescriptor = {
  name: "echo",
  id: SESSION_ECHO_METHOD_ID,
};

describe("session", () => {
  // r[verify connection.protocol]
  // r[verify connection.handshake]
  // r[verify connection.handshake.phon]
  // r[verify connection.handshake.protocol-schema]
  // r[verify connection.handshake.protocol-schema.connection-scoped]
  // r[verify connection.handshake.unversioned]
  // r[verify lane.settings]
  // r[verify connection.handshake.lane-settings]
  // r[verify connection.peer]
  // r[verify connection.role]
  // r[verify connection.symmetry]
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

  // r[verify connection.handshake.sorry]
  // r[verify connection.handshake.protocol-schema]
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

  // r[verify connection.handshake.sorry]
  // r[verify connection.handshake.protocol-schema]
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

  // r[verify rpc.observability.establishment]
  it("reports connection establishment phases over transport prologue", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientEvents: EstablishmentEvent[] = [];
    const serverEvents: EstablishmentEvent[] = [];
    const [clientSession, serverSession] = await withTimeout(
      Promise.all([
        connect(clientLink, { observer: establishmentObserver(clientEvents) }),
        accept(serverLink, { observer: establishmentObserver(serverEvents) }),
      ]),
      "observed transport establishment",
    );

    expect(establishmentLabels(clientEvents)).toEqual([
      "started:initiator:transport-prologue:-:-",
      "finished:initiator:transport-prologue:-:ok",
      "started:initiator:connection-handshake:-:-",
      "finished:initiator:connection-handshake:-:ok",
      "started:initiator:schema-decode-plan:-:-",
      "finished:initiator:schema-decode-plan:-:ok",
    ]);
    expect(establishmentLabels(serverEvents)).toEqual([
      "started:acceptor:transport-prologue:-:-",
      "finished:acceptor:transport-prologue:-:ok",
      "started:acceptor:connection-handshake:-:-",
      "finished:acceptor:connection-handshake:-:ok",
      "started:acceptor:schema-decode-plan:-:-",
      "finished:acceptor:schema-decode-plan:-:ok",
    ]);

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed(), serverSession.closed()]);
  });

  // r[verify rpc.observability.establishment]
  it("does not invent transport prologue phases for already-open links", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientEvents: EstablishmentEvent[] = [];
    const serverEvents: EstablishmentEvent[] = [];
    const [clientSession, serverSession] = await withTimeout(
      Promise.all([
        connectOnLink(clientLink, { observer: establishmentObserver(clientEvents) }),
        acceptOnLink(serverLink, { observer: establishmentObserver(serverEvents) }),
      ]),
      "observed direct-link establishment",
    );

    expect(establishmentLabels(clientEvents)).toEqual([
      "started:initiator:connection-handshake:-:-",
      "finished:initiator:connection-handshake:-:ok",
      "started:initiator:schema-decode-plan:-:-",
      "finished:initiator:schema-decode-plan:-:ok",
    ]);
    expect(establishmentLabels(serverEvents)).toEqual([
      "started:acceptor:connection-handshake:-:-",
      "finished:acceptor:connection-handshake:-:ok",
      "started:acceptor:schema-decode-plan:-:-",
      "finished:acceptor:schema-decode-plan:-:ok",
    ]);

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed(), serverSession.closed()]);
  });

  // r[verify connection.lane-id-parity]
  // r[verify lane.service.compat]
  // r[verify lane.open.wire]
  // r[verify lane.open.api]
  it("allocates service lane ids from local session parity", async () => {
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
    const initiatorFirst = initiatorSession.handle().openLane(requestedSettings);
    const initiatorFirstOpen = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "initiator first connection open"))!,
    );
    expect(initiatorFirstOpen.lane_id).toBe(1n);
    expect(initiatorFirstOpen.payload.tag).toBe("LaneOpen");
    await rawServerLink.send(encodeMessage(messageLaneAccept(1n, acceptedSettings)));
    await withTimeout(initiatorFirst, "initiator first connection accept");

    const initiatorSecond = initiatorSession.handle().openLane(requestedSettings);
    const initiatorSecondOpen = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "initiator second connection open"))!,
    );
    expect(initiatorSecondOpen.lane_id).toBe(3n);
    expect(initiatorSecondOpen.payload.tag).toBe("LaneOpen");
    await rawServerLink.send(encodeMessage(messageLaneAccept(3n, acceptedSettings)));
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
    const acceptorFirst = acceptorSession.handle().openLane(requestedSettings);
    const acceptorFirstOpen = decodeMessage(
      (await withTimeout(rawClientLink.recv(), "acceptor first connection open"))!,
    );
    expect(acceptorFirstOpen.lane_id).toBe(2n);
    expect(acceptorFirstOpen.payload.tag).toBe("LaneOpen");
    await rawClientLink.send(encodeMessage(messageLaneAccept(2n, acceptedSettings)));
    await withTimeout(acceptorFirst, "acceptor first connection accept");

    const acceptorSecond = acceptorSession.handle().openLane(requestedSettings);
    const acceptorSecondOpen = decodeMessage(
      (await withTimeout(rawClientLink.recv(), "acceptor second connection open"))!,
    );
    expect(acceptorSecondOpen.lane_id).toBe(4n);
    expect(acceptorSecondOpen.payload.tag).toBe("LaneOpen");
    await rawClientLink.send(encodeMessage(messageLaneAccept(4n, acceptedSettings)));
    await withTimeout(acceptorSecond, "acceptor second connection accept");

    rawClientLink.close();
    acceptorLink.close();
    acceptorSession.handle().shutdown();
    await Promise.allSettled([acceptorSession.closed()]);
  });

  // r[verify lane.service.compat]
  // r[verify lane.accept.api]
  // r[verify lane]
  // r[verify lane.open]
  // r[verify lane.wire.compat]
  // r[verify connection.symmetry]
  // r[verify lane.open.settings]
  // r[verify connection.message]
  // r[verify connection.message.lane-id]
  // r[verify connection.message.payloads]
  // r[verify rpc.request]
  // r[verify rpc.response]
  it("accepts inbound service lanes and routes calls on them", async () => {
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [clientLink, rawServerLink] = memoryLinkPair();
    let acceptConnection!: (connection: Lane) => void;
    const acceptedConnection = new Promise<Lane>((resolve) => {
      acceptConnection = resolve;
    });
    const initiatorSession = await withTimeout(
      establishRawInitiator(clientLink, rawServerLink, {
        onLane: (connection) => acceptConnection(connection),
      }),
      "raw initiator establishment",
    );

    await rawServerLink.send(
      encodeMessage(messageLaneOpen(2n, peerSettings, emptyMetadata())),
    );
    const accept = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane accept"))!,
    );
    expect(accept.lane_id).toBe(2n);
    expect(accept.payload.tag).toBe("LaneAccept");
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
      (await withTimeout(rawServerLink.recv(), "service lane response"))!,
    );
    expect(response.lane_id).toBe(2n);
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

  // r[verify lane.open.wire.rejection]
  // r[verify lane.open.result]
  it("rejects inbound service lanes when no acceptor is configured", async () => {
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
      encodeMessage(messageLaneOpen(2n, peerSettings, emptyMetadata())),
    );
    const reject = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane reject"))!,
    );
    expect(reject.lane_id).toBe(2n);
    expect(reject.payload.tag).toBe("LaneReject");
    if (reject.payload.tag === "LaneReject") {
      const rejection = LaneRejection.fromMetadata(
        coerceMetadata(reject.payload.value.metadata),
      );
      expect(rejection.reason).toBe("not-ready");
      expect(rejection.message()).toBe("no lane acceptor configured");
    }

    clientLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify lane.open.result]
  it("surfaces peer lane-open rejection metadata to callers", async () => {
    const requestedSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink),
      "raw initiator establishment",
    );

    const opened = initiatorSession.handle()
      .openLane(requestedSettings)
      .catch((error: unknown) => error);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane open"))!,
    );
    expect(open.payload.tag).toBe("LaneOpen");

    await rawServerLink.send(
      encodeMessage(
        messageLaneReject(
          open.lane_id,
          LaneRejection.withMessage("unknown-service", "service is hidden").toMetadata(),
        ),
      ),
    );

    const error = await opened;
    expect(error).toBeInstanceOf(ConnectionError);
    const connectionError = error as ConnectionError;
    expect(connectionError.message).toBe(
      "lane open rejected: unknown-service: service is hidden",
    );
    expect(connectionError.rejection?.reason).toBe("unknown-service");
    expect(connectionError.rejection?.message()).toBe("service is hidden");

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify rpc.observability.establishment]
  it("reports service lane open accept and reject outcomes", async () => {
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
    const events: EstablishmentEvent[] = [];
    const [initiatorLink, rawServerLink] = memoryLinkPair();
    const initiatorSession = await withTimeout(
      establishRawInitiator(initiatorLink, rawServerLink, {
        observer: establishmentObserver(events),
      }),
      "raw observed initiator establishment",
    );

    const accepted = initiatorSession.handle().openLane(requestedSettings);
    const acceptedOpen = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "observed service lane open"))!,
    );
    expect(acceptedOpen.payload.tag).toBe("LaneOpen");
    await rawServerLink.send(
      encodeMessage(messageLaneAccept(acceptedOpen.lane_id, acceptedSettings)),
    );
    await withTimeout(accepted, "observed service lane accept");

    const rejected = initiatorSession.handle()
      .openLane(requestedSettings)
      .catch((error: unknown) => error);
    const rejectedOpen = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "observed service lane rejected open"))!,
    );
    expect(rejectedOpen.payload.tag).toBe("LaneOpen");
    await rawServerLink.send(
      encodeMessage(
        messageLaneReject(
          rejectedOpen.lane_id,
          LaneRejection.withMessage("unknown-service", "missing").toMetadata(),
        ),
      ),
    );
    await expect(withTimeout(rejected, "observed service lane reject")).resolves
      .toBeInstanceOf(ConnectionError);

    const serviceLaneLabels = establishmentLabels(
      events.filter((event) => event.context.phase === "service-lane-open"),
    );
    expect(serviceLaneLabels).toEqual([
      "started:initiator:service-lane-open:1:-",
      "finished:initiator:service-lane-open:1:ok",
      "started:initiator:service-lane-open:3:-",
      "finished:initiator:service-lane-open:3:rejected",
    ]);

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify rpc.flow-control.credit.initial.high-level]
  // r[verify rpc.flow-control.credit.initial]
  // r[verify rpc.flow-control.credit.initial.zero]
  // r[verify rpc.flow-control.max-concurrent-requests.default]
  // r[verify lane.control.compat]
  it("applies and rejects initial lane capacity settings", () => {
    expect(defaultLaneSettings(Role.Acceptor)).toMatchObject({
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    });
    expect(defaultLaneSettings(Role.Initiator, 64, 7)).toMatchObject({
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 7,
    });
    expect(() => defaultLaneSettings(Role.Acceptor, 64, 0)).toThrow(/initial_channel_credit/);
  });

  // r[verify transport.prologue.first-payload]
  // r[verify transport.prologue.post-accept]
  // r[verify conduit]
  // r[verify conduit.bare]
  // r[verify conduit.typeplan]
  // r[verify lane.id.compat]
  // r[verify connection.model]
  // r[verify connection.lifecycle.driven]
  // r[verify lane.control.compat]
  // r[verify lane.control]
  it("establishes over transport prologue before BareConduit traffic", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const [clientSession, serverSession] = await withTimeout(
      Promise.all([
        connect(clientLink),
        accept(serverLink),
      ]),
      "transport session establishment",
    );
    expect("lane" in serverSession).toBe(false);
    await expect(serverSession.handle().closeLane(0n)).rejects.toThrow(
      /cannot close the initial lane/,
    );

    await clientLink.send(
      encodeMessage(
        messageRequest(1n, ECHO_METHOD.id, new Uint8Array(), emptyMetadata(), [], 0n, []),
      ),
    );

    await withTimeout(serverSession.closed(), "server protocol-error close");

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed(), serverSession.closed()]);
  });

  // r[verify connection.keepalive]
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
      lane_id: 0n,
      payload: { tag: "Pong", value: { nonce: 123n } },
    });

    clientLink.close();
    serverLink.close();
    serverSession.handle().shutdown();
    await Promise.allSettled([serverSession.closed()]);
  });

  // r[verify connection.keepalive]
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

  // r[verify connection.protocol-error]
  // r[verify rpc.observability.connection-errors]
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
    expect(message.lane_id).toBe(0n);
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
    const connection = new Lane(
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

    const callBodies = sent.flatMap((message) => {
      expect(message.payload.tag).toBe("RequestMessage");
      const body = message.payload.tag === "RequestMessage"
        ? message.payload.value.body
        : undefined;
      return body?.tag === "Call" ? [body] : [];
    });
    const schemas = callBodies.map((body) => {
      expect(body.tag).toBe("Call");
      return body.value.schemas;
    });

    expect(schemas).toEqual([
      Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.argsSchemaClosure)),
      [],
    ]);
  });

  // r[verify rpc.timeout.idle-progress]
  it("rejects request-idle calls with RpcError and sends cancel", async () => {
    vi.useFakeTimers();
    try {
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
      const connection = new Lane(
        fakeSession as never,
        0n,
        settings,
        settings,
      );

      const call = connection.caller().call({
        method: "Test.echo",
        args: { value: 55 },
        descriptor: ECHO_METHOD,
        methodSchemas: ECHO_METHOD_SCHEMAS,
        registry: sessionEchoRegistry,
        timeoutMs: 10,
      });
      const observedCall = call.catch((error: unknown) => error);
      await Promise.resolve();
      await Promise.resolve();

      await vi.advanceTimersByTimeAsync(10);

      const thrown = await observedCall;
      expect(thrown).toBeInstanceOf(RpcError);
      expect((thrown as RpcError).code).toBe(RpcErrorCode.TIMED_OUT);
      const requests = sent
        .filter((message) => message.payload.tag === "RequestMessage")
        .map((message) => message.payload.tag === "RequestMessage" ? message.payload.value : null);
      expect(requests.map((request) => request?.id)).toEqual([1n, 1n]);
      expect(requests.map((request) => request?.body.tag)).toEqual(["Call", "Cancel"]);
    } finally {
      vi.useRealTimers();
    }
  });

  // r[verify rpc.timeout.idle-progress]
  it("extends request idle timeout on request-associated channel activity", async () => {
    vi.useFakeTimers();
    try {
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
      const connection = new Lane(
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
        timeoutMs: 80,
        channels: [7n],
      });
      await Promise.resolve();
      await Promise.resolve();

      await vi.advanceTimersByTimeAsync(50);
      connection.routeChannelData(7n, Uint8Array.of(1));
      await vi.advanceTimersByTimeAsync(31);

      const payload = encodeTyped(
        { tag: "Ok", value: 55 } as never,
        ECHO_METHOD_SCHEMAS.responseRoot,
        sessionEchoRegistry,
      );
      connection.resolveResponse(1n, payload);

      await expect(call).resolves.toBe(55);
      const requests = sent
        .filter((message) => message.payload.tag === "RequestMessage")
        .map((message) => message.payload.tag === "RequestMessage" ? message.payload.value : null);
      expect(requests.map((request) => request?.body.tag)).toEqual(["Call"]);
    } finally {
      vi.useRealTimers();
    }
  });

  // r[verify lane.request-channel-parity]
  // r[verify connection.message]
  // r[verify connection.message.lane-id]
  // r[verify connection.message.payloads]
  // r[verify rpc.request]
  // r[verify rpc.request.id-allocation]
  it("allocates request ids from service lane parity", async () => {
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
    const connection = new Lane(
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

    const calls = sent.flatMap((message) => {
      expect(message.lane_id).toBe(13n);
      expect(message.payload.tag).toBe("RequestMessage");
      const requestMessage = message.payload.tag === "RequestMessage"
        ? message.payload.value
        : undefined;
      return requestMessage?.body.tag === "Call" ? [requestMessage.id] : [];
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
    const connection = new Lane(
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
      laneId: 0n,
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

    connection.close(ConnectionError.closed());
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
    const connection = new Lane(
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

    connection.close(ConnectionError.closed());
    await expect(call).rejects.toBeInstanceOf(ConnectionError);
  });

  // r[verify rpc.request.scope]
  // r[verify rpc.request.scope.channels]
  // r[verify rpc.request.scope.terminal]
  it("terminalizes caller receive channels when a response is delivered", async () => {
    const localSettings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const peerSettings: ConnectionSettings = {
      parity: { tag: "Even" },
      max_concurrent_requests: 64,
      initial_channel_credit: 16,
    };
    const fakeSession = {
      sendMessage: async () => {},
    };
    const connection = new Lane(
      fakeSession as never,
      0n,
      localSettings,
      peerSettings,
    );
    const [tx, rx] = channel<number>();
    const serverSendsSchemas: PhonMethodSchemas = {
      ...CHANNEL_METHOD_SCHEMAS,
      channels: [{ index: 0, direction: "tx", elementRoot: primitiveId("u32") }],
    };

    const call = connection.caller().call({
      method: "Test.stream",
      args: { numbers: tx },
      descriptor: CHANNEL_METHOD,
      methodSchemas: serverSendsSchemas,
      registry: CHANNEL_REGISTRY,
      timeoutMs: 1_000,
    });
    const observedCall = call.catch((error) => error);
    await new Promise((resolve) => setTimeout(resolve, 0));

    connection.resolveResponse(1n, Uint8Array.of(0, 0, 0, 0));

    await expect(rx.recv()).rejects.toMatchObject({ kind: "requestClosed" });
    await observedCall;
  });

  // r[verify rpc.cancel]
  // r[verify rpc.cancel.channels]
  // r[verify rpc.request.scope.terminal]
  it("queues inbound cancel and terminalizes request channels", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const { serverSession, serverLane, laneId } = await establishInboundServiceLane(
      clientLink,
      serverLink,
    );
    const schemas = Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.argsSchemaClosure));

    await clientLink.send(
      encodeMessage(
        messageRequest(
          1n,
          ECHO_METHOD.id,
          new Uint8Array(),
          emptyMetadata(),
          [11n],
          laneId,
          schemas,
        ),
      ),
    );
    const incoming = await withTimeout(serverLane.nextIncomingCall(), "incoming cancellable call");
    expect(incoming?.requestId).toBe(1n);
    expect(incoming?.channels).toEqual([11n]);

    const receiver = serverLane.getChannelRegistry().registerIncoming(11n, 4);
    await clientLink.send(encodeMessage(messageCancel(1n, laneId)));
    expect(await withTimeout(serverLane.nextIncomingCancel(), "incoming cancel")).toBe(1n);
    expect(serverLane.debugSnapshot()).toMatchObject({
      closed: false,
      inboundLiveRequestCount: 0,
    });

    await expect(withTimeout(receiver.recv(), "post-cancel channel terminal"))
      .rejects.toMatchObject({ kind: "cancelled" });

    await serverLane.sendResponse(1n, Uint8Array.of(9));
    const lateResponse = decodeMessage(
      (await withTimeout(clientLink.recv(), "post-cancel response"))!,
    );
    expect(lateResponse.lane_id).toBe(laneId);
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

  // r[verify rpc.flow-control.max-concurrent-requests.connection-failure]
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
    const connection = new Lane(
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
    connection.close(ConnectionError.closed());

    await expect(first).rejects.toBeInstanceOf(ConnectionError);
    await expect(second).rejects.toBeInstanceOf(ConnectionError);
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
    expect(message.lane_id).toBe(0n);
    expect(message.payload.tag).toBe("ProtocolError");
    if (message.payload.tag === "ProtocolError") {
      expect(message.payload.value.description).toContain("max_concurrent_requests");
    }
    await withTimeout(serverSession.closed(), "max-concurrent teardown");

    clientLink.close();
    serverLink.close();
  });

  // r[verify rpc.caller.liveness.last-drop-closes-connection]
  it("does not close a service lane when its last caller is disposed", async () => {
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

    const opened = initiatorSession.handle().openLane(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane open"))!,
    );
    expect(open.lane_id).toBe(1n);
    await rawServerLink.send(encodeMessage(messageLaneAccept(1n, peerSettings)));
    const connection = await withTimeout(opened, "service lane accept");

    const caller = connection.caller();
    caller.dispose();

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(rawServerLink.queuedPayloadCount()).toBe(0);
    expect(connection.isClosed()).toBe(false);

    await initiatorSession.handle().closeLane(open.lane_id);
    const close = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "explicit service lane close"))!,
    );
    expect(close.lane_id).toBe(1n);
    expect(close.payload.tag).toBe("LaneClose");

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify rpc.caller.liveness.refcounted]
  it("disposing service lane callers only releases local references", async () => {
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

    const opened = initiatorSession.handle().openLane(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane open"))!,
    );
    await rawServerLink.send(encodeMessage(messageLaneAccept(open.lane_id, peerSettings)));
    const connection = await withTimeout(opened, "service lane accept");

    const firstCaller = connection.caller();
    const secondCaller = connection.caller();

    firstCaller.dispose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(rawServerLink.queuedPayloadCount()).toBe(0);
    expect(connection.isClosed()).toBe(false);

    secondCaller.dispose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(rawServerLink.queuedPayloadCount()).toBe(0);
    expect(connection.isClosed()).toBe(false);

    await initiatorSession.handle().closeLane(open.lane_id);
    const close = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "explicit service lane close"))!,
    );
    expect(close.lane_id).toBe(open.lane_id);
    expect(close.payload.tag).toBe("LaneClose");
    expect(connection.isClosed()).toBe(true);

    initiatorLink.close();
    rawServerLink.close();
    initiatorSession.handle().shutdown();
    await Promise.allSettled([initiatorSession.closed()]);
  });

  // r[verify lane.close]
  // r[verify lane.close.semantics]
  it("tears down a service lane after receiving close", async () => {
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

    const opened = initiatorSession.handle().openLane(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane open"))!,
    );
    await rawServerLink.send(encodeMessage(messageLaneAccept(open.lane_id, peerSettings)));
    const connection = await withTimeout(opened, "service lane accept");
    expect(connection.isClosed()).toBe(false);

    await rawServerLink.send(encodeMessage(messageLaneClose(open.lane_id)));
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
          open.lane_id,
          [],
        ),
      ),
    );
    const errorPayload = await withTimeout(rawServerLink.recv(), "post-close protocol error");
    expect(errorPayload).not.toBeNull();
    const errorMessage = decodeMessage(errorPayload!);
    expect(errorMessage.lane_id).toBe(0n);
    expect(errorMessage.payload.tag).toBe("ProtocolError");
    if (errorMessage.payload.tag === "ProtocolError") {
      expect(errorMessage.payload.value.description).toContain(
        `unknown lane ${open.lane_id}`,
      );
    }

    await withTimeout(initiatorSession.closed(), "post-close protocol teardown");
    rawServerLink.close();
  });

  // r[verify connection.shutdown.explicit]
  // r[verify lane.control]
  // r[verify rpc.caller.liveness.root-internal-close]
  // r[verify rpc.caller.liveness.root-teardown-condition]
  it("does not expose the internal control lane as a public service lane", async () => {
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

    const opened = initiatorSession.handle().openLane(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane open"))!,
    );
    await rawServerLink.send(encodeMessage(messageLaneAccept(open.lane_id, peerSettings)));
    const connection = await withTimeout(opened, "service lane accept");

    const virtualCaller = connection.caller();

    expect("lane" in initiatorSession).toBe(false);

    virtualCaller.dispose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(rawServerLink.queuedPayloadCount()).toBe(0);
    expect(initiatorLink.isClosed()).toBe(false);

    initiatorSession.handle().shutdown();
    await withTimeout(initiatorSession.closed(), "explicit shutdown teardown");
    expect(initiatorLink.isClosed()).toBe(true);

    rawServerLink.close();
  });

  // r[verify schema.exchange.required]
  it("tears down when a call arrives without an args schema binding", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const { serverSession, serverLane, laneId } = await establishInboundServiceLane(
      clientLink,
      serverLink,
    );

    await clientLink.send(
      encodeMessage(
        messageRequest(1n, ECHO_METHOD.id, new Uint8Array(), emptyMetadata(), [], laneId, []),
      ),
    );

    await withTimeout(serverSession.closed(), "server protocol-error close");
    expect(serverLane.isClosed()).toBe(true);

    clientLink.close();
    serverLink.close();
    serverSession.handle().shutdown();
    await Promise.allSettled([serverSession.closed()]);
  });

  // r[verify schema.exchange.required]
  it("tears down when a response arrives without a response schema binding", async () => {
    const [clientLink, rawServerLink] = memoryLinkPair();
    const clientSession = await withTimeout(
      establishRawInitiator(clientLink, rawServerLink),
      "raw initiator establishment",
    );
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
    const opened = clientSession.handle().openLane(settings);
    const open = decodeMessage(
      (await withTimeout(rawServerLink.recv(), "service lane open"))!,
    );
    await rawServerLink.send(encodeMessage(messageLaneAccept(open.lane_id, peerSettings)));
    const clientLane = await withTimeout(opened, "service lane accept");

    const call = clientLane.caller().call({
      method: "Test.echo",
      args: { value: 55 },
      descriptor: ECHO_METHOD,
      methodSchemas: ECHO_METHOD_SCHEMAS,
      registry: sessionEchoRegistry,
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    await rawServerLink.send(
      encodeMessage(messageResponse(1n, new Uint8Array(), emptyMetadata(), open.lane_id, [])),
    );

    await withTimeout(clientSession.closed(), "client protocol-error close");
    await expect(call).rejects.toBeInstanceOf(ConnectionError);
    expect(clientLane.isClosed()).toBe(true);

    clientLink.close();
    rawServerLink.close();
    clientSession.handle().shutdown();
    await Promise.allSettled([clientSession.closed()]);
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
    const connection = new Lane(
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
      const connection = new Lane(
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
    const connection = new Lane(
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
      lane_id: 0n,
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
