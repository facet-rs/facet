import { describe, expect, it } from "vitest";
import type { ConnectionSettings, Message } from "@bearcove/roam-wire";
import { BareConduit } from "./conduit.ts";
import { Driver, type Dispatcher } from "./driver.ts";
import { handshakeAsAcceptor, handshakeAsInitiator } from "./handshake.ts";
import { Extensions } from "./middleware.ts";
import { RequestContext } from "./request_context.ts";
import { session } from "./session.ts";
import { ClientMetadata } from "./metadata.ts";
import { OPERATION_ID_METADATA_KEY } from "./retry.ts";
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

const METHOD: MethodDescriptor = {
  name: "echo",
  id: 1n,
  retry: { persist: true, idem: false },
};

const UNIT_ID = 10n;
const U32_ID = 11n;
const STRING_ID = 12n;
const RESULT_ID = 13n;
const ROAM_ERROR_ID = 14n;
const U32_ARGS_ID = 15n;

const ECHO_SEND_SCHEMAS: ServiceSendSchemas = {
  schemas: new Map([
    [UNIT_ID, { id: UNIT_ID, type_params: [], kind: { tag: "primitive", primitive_type: "unit" } }],
    [U32_ID, { id: U32_ID, type_params: [], kind: { tag: "primitive", primitive_type: "u32" } }],
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
      ROAM_ERROR_ID,
      {
        id: ROAM_ERROR_ID,
        type_params: ["E"],
        kind: {
          tag: "enum",
          name: "RoamError",
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
    [STRING_ID, { id: STRING_ID, type_params: [], kind: { tag: "primitive", primitive_type: "string" } }],
  ]),
  methods: new Map([
    [
      METHOD.id,
      {
        argsRootRef: { tag: "concrete", type_id: U32_ARGS_ID, args: [] },
        responseRootRef: {
          tag: "concrete",
          type_id: RESULT_ID,
          args: [
            { tag: "concrete", type_id: U32_ID, args: [] },
            {
              tag: "concrete",
              type_id: ROAM_ERROR_ID,
              args: [{ tag: "concrete", type_id: UNIT_ID, args: [] }],
            },
          ],
        },
      },
    ],
  ]),
};

const DESCRIPTOR: ServiceDescriptor = {
  service_name: "Test",
  send_schemas: ECHO_SEND_SCHEMAS,
  methods: [METHOD],
};

const CANONICAL_ZERO_ARG_METHOD: MethodDescriptor = {
  name: "ping",
  id: 2n,
  retry: { persist: false, idem: false },
};

const CANONICAL_ZERO_ARG_SEND_SCHEMAS: ServiceSendSchemas = {
  schemas: new Map([
    [UNIT_ID, { id: UNIT_ID, type_params: [], kind: { tag: "primitive", primitive_type: "unit" } }],
    [U32_ID, { id: U32_ID, type_params: [], kind: { tag: "primitive", primitive_type: "u32" } }],
    [STRING_ID, { id: STRING_ID, type_params: [], kind: { tag: "primitive", primitive_type: "string" } }],
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
      ROAM_ERROR_ID,
      {
        id: ROAM_ERROR_ID,
        type_params: ["E"],
        kind: {
          tag: "enum",
          name: "RoamError",
          variants: [
            {
              name: "User",
              index: 0,
              payload: { tag: "newtype", type_ref: { tag: "var", name: "E" } },
            },
            { name: "UnknownMethod", index: 1, payload: { tag: "unit" } },
            {
              name: "InvalidPayload",
              index: 2,
              payload: { tag: "newtype", type_ref: { tag: "concrete", type_id: STRING_ID, args: [] } },
            },
            { name: "Cancelled", index: 3, payload: { tag: "unit" } },
            { name: "ConnectionClosed", index: 4, payload: { tag: "unit" } },
            { name: "SessionShutdown", index: 5, payload: { tag: "unit" } },
            { name: "SendFailed", index: 6, payload: { tag: "unit" } },
            { name: "Indeterminate", index: 7, payload: { tag: "unit" } },
          ],
        },
      },
    ],
  ]),
  methods: new Map([
    [
      CANONICAL_ZERO_ARG_METHOD.id,
      {
        argsRootRef: { tag: "concrete", type_id: UNIT_ID, args: [] },
        responseRootRef: {
          tag: "concrete",
          type_id: RESULT_ID,
          args: [
            { tag: "concrete", type_id: U32_ID, args: [] },
            {
              tag: "concrete",
              type_id: ROAM_ERROR_ID,
              args: [{ tag: "concrete", type_id: UNIT_ID, args: [] }],
            },
          ],
        },
      },
    ],
  ]),
};

const CANONICAL_ZERO_ARG_DESCRIPTOR: ServiceDescriptor = {
  service_name: "Test",
  methods: [CANONICAL_ZERO_ARG_METHOD],
  send_schemas: CANONICAL_ZERO_ARG_SEND_SCHEMAS,
};

describe("retry operation identity", () => {
  it("normalizes canonical zero-arg methods to an empty arg list", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientConduit = new BareConduit(clientLink);
    const serverConduit = new BareConduit(serverLink);
    const seen = makeDeferred<number>();

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return CANONICAL_ZERO_ARG_DESCRIPTOR;
      },
      async dispatch(_context: RequestContext, _method, args, call) {
        seen.resolve(args.length);
        call.reply(7);
      },
    };

    const clientSettings: ConnectionSettings = { parity: { tag: "Odd" }, max_concurrent_requests: 64 };
    const serverSettings: ConnectionSettings = { parity: { tag: "Even" }, max_concurrent_requests: 64 };
    const [clientHandshake, serverHandshake] = await Promise.all([
      handshakeAsInitiator(clientLink, clientSettings),
      handshakeAsAcceptor(serverLink, serverSettings),
    ]);
    const clientSession = session.initiatorConduit(clientConduit, clientHandshake);
    const serverSession = session.acceptorConduit(serverConduit, serverHandshake);
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const clientDriver = new Driver(clientSession.rootConnection(), {
      getDescriptor: () => CANONICAL_ZERO_ARG_DESCRIPTOR,
      dispatch: async () => {},
    });
    const serverRun = serverDriver.run();
    const clientRun = clientDriver.run();

    const value = await clientSession.rootConnection().caller().call({
      method: "Test.ping",
      args: {},
      descriptor: CANONICAL_ZERO_ARG_METHOD,
      sendSchemas: CANONICAL_ZERO_ARG_SEND_SCHEMAS,
    });

    await expect(seen.promise).resolves.toBe(0);
    expect(value).toBe(7);

    clientLink.close();
    serverLink.close();
    await Promise.allSettled([serverRun, clientRun]);
  });

  it("automatically injects an operation id when the peer supports retry", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientConduit = new BareConduit(clientLink);
    const serverConduit = new BareConduit(serverLink);
    const seen = makeDeferred<bigint>();

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return DESCRIPTOR;
      },
      async dispatch(context: RequestContext, _method, args, call) {
        seen.resolve(context.metadata.get(OPERATION_ID_METADATA_KEY) as bigint);
        call.reply(args[0]);
      },
    };

    const clientSettings: ConnectionSettings = { parity: { tag: "Odd" }, max_concurrent_requests: 64 };
    const serverSettings: ConnectionSettings = { parity: { tag: "Even" }, max_concurrent_requests: 64 };
    const [clientHandshake, serverHandshake] = await Promise.all([
      handshakeAsInitiator(clientLink, clientSettings),
      handshakeAsAcceptor(serverLink, serverSettings),
    ]);
    const clientSession = session.initiatorConduit(clientConduit, clientHandshake);
    const serverSession = session.acceptorConduit(serverConduit, serverHandshake);
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const clientDriver = new Driver(clientSession.rootConnection(), {
      getDescriptor: () => DESCRIPTOR,
      dispatch: async () => {},
    });
    const serverRun = serverDriver.run();
    const clientRun = clientDriver.run();

    const value = await clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 7 },
      descriptor: METHOD,
      metadata: new ClientMetadata(),
      sendSchemas: ECHO_SEND_SCHEMAS,
    });

    await expect(seen.promise).resolves.toBeTypeOf("bigint");
    expect(value).toBe(7);

    clientLink.close();
    serverLink.close();
    await Promise.allSettled([serverRun, clientRun]);
  });

  it("attaches live duplicates and replays sealed outcomes for the same operation id", async () => {
    const [clientLink, serverLink] = memoryLinkPair();
    const clientConduit = new BareConduit(clientLink);
    const serverConduit = new BareConduit(serverLink);
    const gate = makeDeferred<void>();
    let runs = 0;

    const dispatcher: Dispatcher = {
      getDescriptor() {
        return DESCRIPTOR;
      },
      async dispatch(_context, _method, args, call) {
        runs += 1;
        await gate.promise;
        call.reply(args[0]);
      },
    };

    const clientSettings: ConnectionSettings = { parity: { tag: "Odd" }, max_concurrent_requests: 64 };
    const serverSettings: ConnectionSettings = { parity: { tag: "Even" }, max_concurrent_requests: 64 };
    const [clientHandshake, serverHandshake] = await Promise.all([
      handshakeAsInitiator(clientLink, clientSettings),
      handshakeAsAcceptor(serverLink, serverSettings),
    ]);
    const clientSession = session.initiatorConduit(clientConduit, clientHandshake);
    const serverSession = session.acceptorConduit(serverConduit, serverHandshake);
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const clientDriver = new Driver(clientSession.rootConnection(), {
      getDescriptor: () => DESCRIPTOR,
      dispatch: async () => {},
    });
    const serverRun = serverDriver.run();
    const clientRun = clientDriver.run();

    const caller = clientSession.rootConnection().caller().with({
      async pre(_ctx, request) {
        request.metadata.set(OPERATION_ID_METADATA_KEY, 99n);
      },
    });

    const first = caller.call({
      method: "Test.echo",
      args: { value: 11 },
      descriptor: METHOD,
      metadata: new ClientMetadata(),
      sendSchemas: ECHO_SEND_SCHEMAS,
    });
    const second = caller.call({
      method: "Test.echo",
      args: { value: 11 },
      descriptor: METHOD,
      metadata: new ClientMetadata(),
      sendSchemas: ECHO_SEND_SCHEMAS,
    });

    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(runs).toBe(1);

    gate.resolve();

    await expect(first).resolves.toBe(11);
    await expect(second).resolves.toBe(11);
    expect(runs).toBe(1);

    const replayed = await caller.call({
      method: "Test.echo",
      args: { value: 11 },
      descriptor: METHOD,
      metadata: new ClientMetadata(),
      sendSchemas: ECHO_SEND_SCHEMAS,
    });
    expect(replayed).toBe(11);
    expect(runs).toBe(1);

    clientLink.close();
    serverLink.close();
    await Promise.allSettled([serverRun, clientRun]);
  });
});
