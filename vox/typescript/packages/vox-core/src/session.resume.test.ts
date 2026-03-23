import { describe, expect, it } from "vitest";
import {
  RpcErrorCode,
  type ConnectionSettings,
  type Message,
} from "@bearcove/vox-wire";
import { BareConduit } from "./conduit.ts";
import { Driver, type Dispatcher } from "./driver.ts";
import { handshakeAsAcceptor, handshakeAsInitiator, type HandshakeResult } from "./handshake.ts";
import { RequestContext } from "./request_context.ts";
import {
  Session,
  ConnectionHandle,
  SessionError,
  SessionRegistry,
  session,
  type SessionAcceptOutcome,
  type SessionHandle,
} from "./session.ts";
import type { MethodDescriptor, ServiceDescriptor } from "./channeling/index.ts";
import type { ServiceSendSchemas } from "./schema_tracker.ts";

class MemoryLink {
  private readonly queue: Uint8Array[] = [];
  private waiting: ((value: Uint8Array | null) => void) | null = null;
  private closed = false;

  constructor(private readonly deliver: (payload: Uint8Array) => void) {}

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

function makeDeferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
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

async function resumeWhenReady(
  handle: SessionHandle,
  link: MemoryLink,
  isInitiator: boolean,
): Promise<void> {
  const settings: ConnectionSettings = {
    parity: isInitiator ? { tag: "Odd" } : { tag: "Even" },
    max_concurrent_requests: 64,
  };
  const resumeKey = handle.sessionResumeKey();
  const handshake = isInitiator
    ? await handshakeAsInitiator(link, settings, true, resumeKey)
    : await handshakeAsAcceptor(link, settings);
  void handshake;
  const conduit = new BareConduit(link);
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      await handle.resume(conduit);
      return;
    } catch (error) {
      if (
        !(error instanceof SessionError)
        || !error.message.includes("resume is only valid while the session is disconnected")
      ) {
        throw error;
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  throw new Error("session never became disconnected");
}

async function establishPair(
  clientLink: MemoryLink,
  serverLink: MemoryLink,
  opts: { resumable?: boolean } = {},
): Promise<[Session, Session]> {
  const clientSettings: ConnectionSettings = { parity: { tag: "Odd" }, max_concurrent_requests: 64 };
  const serverSettings: ConnectionSettings = { parity: { tag: "Even" }, max_concurrent_requests: 64 };
  const [clientHandshake, serverHandshake] = await Promise.all([
    handshakeAsInitiator(clientLink, clientSettings),
    handshakeAsAcceptor(serverLink, serverSettings, true, opts.resumable ?? false),
  ]);
  const clientConduit = new BareConduit(clientLink);
  const serverConduit = new BareConduit(serverLink);
  const clientSession = session.initiatorConduit(clientConduit, clientHandshake, { resumable: opts.resumable ?? false });
  const serverSession = session.acceptorConduit(serverConduit, serverHandshake, { resumable: opts.resumable ?? false });
  return [clientSession, serverSession];
}

const UNIT_ID = 10n;
const U32_ID = 11n;
const STRING_ID = 12n;
const RESULT_ID = 13n;
const VOX_ERROR_ID = 14n;
const U32_ARGS_ID = 15n;

const ECHO_SEND_SCHEMAS: ServiceSendSchemas = {
  schemas: new Map([
    [UNIT_ID, { id: UNIT_ID, type_params: [], kind: { tag: "primitive", primitive_type: "unit" } }],
    [U32_ID, { id: U32_ID, type_params: [], kind: { tag: "primitive", primitive_type: "u32" } }],
    [STRING_ID, { id: STRING_ID, type_params: [], kind: { tag: "primitive", primitive_type: "string" } }],
    [
      U32_ARGS_ID,
      {
        id: U32_ARGS_ID,
        type_params: [],
        kind: {
          tag: "tuple",
          elements: [{ tag: "concrete", type_id: U32_ID, args: [] }],
        },
      },
    ],
    [
      RESULT_ID,
      {
        id: RESULT_ID,
        type_params: ["T", "E"],
        kind: {
          tag: "enum",
          name: "Result",
          variants: [
            {
              name: "Ok",
              index: 0,
              payload: { tag: "newtype", type_ref: { tag: "var", name: "T" } },
            },
            {
              name: "Err",
              index: 1,
              payload: { tag: "newtype", type_ref: { tag: "var", name: "E" } },
            },
          ],
        },
      },
    ],
    [
      VOX_ERROR_ID,
      {
        id: VOX_ERROR_ID,
        type_params: ["E"],
        kind: {
          tag: "enum",
          name: "VoxError",
          variants: [
            {
              name: "User",
              index: 0,
              payload: { tag: "newtype", type_ref: { tag: "var", name: "E" } },
            },
            { name: "UnknownMethod", index: 1, payload: { tag: "unit" } },
            { name: "InvalidPayload", index: 2, payload: { tag: "newtype", type_ref: { tag: "concrete", type_id: STRING_ID, args: [] } } },
            { name: "Cancelled", index: 3, payload: { tag: "unit" } },
            { name: "Indeterminate", index: 4, payload: { tag: "unit" } },
          ],
        },
      },
    ],
  ]),
  methods: new Map([
    [
      1n,
      {
        argsRootRef: { tag: "concrete", type_id: U32_ARGS_ID, args: [] },
        responseRootRef: {
          tag: "concrete",
          type_id: RESULT_ID,
          args: [
            { tag: "concrete", type_id: U32_ID, args: [] },
            {
              tag: "concrete",
              type_id: VOX_ERROR_ID,
              args: [{ tag: "concrete", type_id: UNIT_ID, args: [] }],
            },
          ],
        },
      },
    ],
  ]),
};

function makeMethod(retry: MethodDescriptor["retry"]): MethodDescriptor {
  return {
    name: "echo",
    id: 1n,
    retry,
  };
}

const PERSIST_METHOD = makeMethod({ persist: true, idem: false });
const IDEM_METHOD = makeMethod({ persist: false, idem: true });
const VOLATILE_METHOD = makeMethod({ persist: false, idem: false });

function descriptorFor(method: MethodDescriptor): ServiceDescriptor {
  return {
    service_name: "Test",
    send_schemas: ECHO_SEND_SCHEMAS,
    methods: new Map([[method.id, method]]),
  };
}

describe("session resumption", () => {
  it("keeps a pending call alive across manual resume on a new conduit", async () => {
    const [clientLink1, serverLink1] = memoryLinkPair();
    const started = makeDeferred<void>();
    const release = makeDeferred<void>();

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return descriptorFor(PERSIST_METHOD);
      },
      async dispatch(_context: RequestContext, _method, args, call) {
        started.resolve();
        await release.promise;
        call.reply(args[0]);
      },
    };

    const [clientSession, serverSession] = await withTimeout(
      establishPair(clientLink1, serverLink1, { resumable: true }),
      "initial session establishment",
    );
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 55 },
      descriptor: PERSIST_METHOD,
      sendSchemas: ECHO_SEND_SCHEMAS,
    });

    await withTimeout(started.promise, "handler start");

    clientLink1.close();
    serverLink1.close();

    const [clientLink2, serverLink2] = memoryLinkPair();

    await withTimeout(
      Promise.all([
        resumeWhenReady(serverSession.handle(), serverLink2, false),
        resumeWhenReady(clientSession.handle(), clientLink2, true),
      ]),
      "session resume",
    );

    release.resolve();

    await expect(withTimeout(call, "resumed call")).resolves.toBe(55);

    clientLink2.close();
    serverLink2.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();

    await Promise.allSettled([serverRun, serverSession.closed(), clientSession.closed()]);
  });

  it("automatically reruns a released idem call after manual resume", async () => {
    const [clientLink1, serverLink1] = memoryLinkPair();
    const firstStarted = makeDeferred<void>();
    const dropFirst = makeDeferred<void>();
    let runs = 0;

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return descriptorFor(IDEM_METHOD);
      },
      async dispatch(_context: RequestContext, _method, args, call) {
        runs += 1;
        if (runs === 1) {
          firstStarted.resolve();
          await dropFirst.promise;
          return;
        }
        call.reply(args[0]);
      },
    };

    const [clientSession, serverSession] = await withTimeout(
      establishPair(clientLink1, serverLink1, { resumable: true }),
      "initial session establishment",
    );
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 77 },
      descriptor: IDEM_METHOD,
      sendSchemas: ECHO_SEND_SCHEMAS,
    });

    await withTimeout(firstStarted.promise, "first handler start");
    clientLink1.close();
    serverLink1.close();
    dropFirst.resolve();

    const [clientLink2, serverLink2] = memoryLinkPair();

    await withTimeout(
      Promise.all([
        resumeWhenReady(serverSession.handle(), serverLink2, false),
        resumeWhenReady(clientSession.handle(), clientLink2, true),
      ]),
      "session resume",
    );

    await expect(withTimeout(call, "retried idem call")).resolves.toBe(77);
    expect(runs).toBe(2);

    clientLink2.close();
    serverLink2.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();

    await Promise.allSettled([serverRun, serverSession.closed(), clientSession.closed()]);
  });

  it("returns indeterminate for a released non-idem call after manual resume", async () => {
    const [clientLink1, serverLink1] = memoryLinkPair();
    const firstStarted = makeDeferred<void>();
    const dropFirst = makeDeferred<void>();
    let runs = 0;

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return descriptorFor(VOLATILE_METHOD);
      },
      async dispatch(_context: RequestContext, _method, _args, _call) {
        runs += 1;
        firstStarted.resolve();
        await dropFirst.promise;
      },
    };

    const [clientSession, serverSession] = await withTimeout(
      establishPair(clientLink1, serverLink1, { resumable: true }),
      "initial session establishment",
    );
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 88 },
      descriptor: VOLATILE_METHOD,
      sendSchemas: ECHO_SEND_SCHEMAS,
    });

    await withTimeout(firstStarted.promise, "first handler start");
    clientLink1.close();
    serverLink1.close();
    dropFirst.resolve();

    const [clientLink2, serverLink2] = memoryLinkPair();

    await withTimeout(
      Promise.all([
        resumeWhenReady(serverSession.handle(), serverLink2, false),
        resumeWhenReady(clientSession.handle(), clientLink2, true),
      ]),
      "session resume",
    );

    await expect(withTimeout(call, "retried non-idem call")).rejects.toMatchObject({
      code: RpcErrorCode.INDETERMINATE,
    });
    expect(runs).toBe(1);

    clientLink2.close();
    serverLink2.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();

    await Promise.allSettled([serverRun, serverSession.closed(), clientSession.closed()]);
  });

  it("fails closed on resume for channel-bearing non-idempotent calls", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const [clientSession, serverSession] = await withTimeout(
      establishPair(clientLink, serverLink, { resumable: true }),
      "initial session establishment",
    );

    const connection = clientSession.rootConnection() as unknown as {
      pendingResponses: Map<bigint, {
        settled: boolean;
        timer: ReturnType<typeof setTimeout>;
        methodId: bigint;
        payload: Uint8Array;
        metadata: [];
        channels: bigint[];
        persist: boolean;
        idem: boolean;
        requestIds: Set<bigint>;
        resolve: (value: Uint8Array) => void;
        reject: (reason: unknown) => void;
        finalizeChannels?: () => void;
      }>;
      onSessionResumed(): void;
    };

    let rejected: unknown;
    connection.pendingResponses.set(1n, {
      settled: false,
      timer: setTimeout(() => {}, 1_000),
      methodId: 1n,
      payload: new Uint8Array(0),
      metadata: [],
      channels: [7n],
      persist: false,
      idem: false,
      requestIds: new Set([1n]),
      resolve: () => {},
      reject: (reason) => {
        rejected = reason;
      },
    });

    connection.onSessionResumed();

    expect(rejected).toMatchObject({ code: RpcErrorCode.INDETERMINATE });
    expect(connection.pendingResponses.size).toBe(0);

    clientLink.close();
    serverLink.close();
    clientSession.handle().shutdown();
    serverSession.handle().shutdown();

    await Promise.allSettled([serverSession.closed(), clientSession.closed()]);
  });

  it("restarts channel flushing when new work arrives during a pending exit", async () => {
    const settings: ConnectionSettings = {
      parity: { tag: "Odd" },
      max_concurrent_requests: 64,
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
      false,
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

  it("keeps a pending call alive across registry-driven acceptor resume", async () => {
    const registry = new SessionRegistry();
    const [clientLink1, serverLink1] = memoryLinkPair();
    const started = makeDeferred<void>();
    const release = makeDeferred<void>();

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return descriptorFor(PERSIST_METHOD);
      },
      async dispatch(_context: RequestContext, _method, args, call) {
        started.resolve();
        await release.promise;
        call.reply(args[0]);
      },
    };

    const clientSettings: ConnectionSettings = { parity: { tag: "Odd" }, max_concurrent_requests: 64 };
    const serverSettings: ConnectionSettings = { parity: { tag: "Even" }, max_concurrent_requests: 64 };
    const [clientHandshake, serverHandshake] = await Promise.all([
      handshakeAsInitiator(clientLink1, clientSettings),
      handshakeAsAcceptor(serverLink1, serverSettings, true, true),
    ]);
    const clientConduit1 = new BareConduit(clientLink1);
    const serverConduit1 = new BareConduit(serverLink1);
    const clientSession = session.initiatorConduit(clientConduit1, clientHandshake, { resumable: true });
    const firstAccepted = session.acceptorOrResume(serverConduit1, serverHandshake, registry, { resumable: true });
    expect((firstAccepted as SessionAcceptOutcome).tag).toBe("Established");
    const firstSession = (firstAccepted as Extract<SessionAcceptOutcome, { tag: "Established" }>).session;
    const serverDriver = new Driver(firstSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 66 },
      descriptor: PERSIST_METHOD,
      sendSchemas: ECHO_SEND_SCHEMAS,
    });

    await withTimeout(started.promise, "handler start");

    clientLink1.close();
    serverLink1.close();

    const [clientLink2, serverLink2] = memoryLinkPair();

    const [serverHandshake2, clientLink2Settled] = await Promise.all([
      handshakeAsAcceptor(serverLink2, serverSettings, true, true),
      resumeWhenReady(clientSession.handle(), clientLink2, true).then(() => null),
    ]);
    void clientLink2Settled;
    const serverConduit2 = new BareConduit(serverLink2);
    const acceptResult = session.acceptorOrResume(serverConduit2, serverHandshake2, registry, { resumable: true });

    expect(acceptResult.tag).toBe("Resumed");

    release.resolve();

    await expect(withTimeout(call, "registry-resumed call")).resolves.toBe(66);

    clientLink2.close();
    serverLink2.close();
    clientSession.handle().shutdown();
    firstSession.handle().shutdown();

    await Promise.allSettled([serverRun, firstSession.closed(), clientSession.closed()]);
  });
});
