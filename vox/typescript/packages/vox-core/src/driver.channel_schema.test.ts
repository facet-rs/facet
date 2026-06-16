import { describe, expect, it } from "vitest";
import { emptyMetadata } from "@bearcove/vox-wire";
import { decodeTyped } from "@bearcove/phon-engine";
import { hexToBytes, type Registry } from "@bearcove/phon-schema";
import { Driver } from "./driver.ts";
import {
  SchemaCompatibilityError,
  SchemaSendTracker,
  type PhonChannelMeta,
  type PhonMethodSchemas,
  type SchemaTracker,
} from "./schema_tracker.ts";
import { ChannelRegistry, type MethodDescriptor, type TaskMessage } from "./channeling/index.ts";
import {
  sessionEchoRegistry,
  sessionEchoMethods,
  SESSION_ECHO_METHOD_ID,
} from "./session_echo.fixture.ts";

const METHOD: MethodDescriptor = {
  name: "stream",
  id: 77n,
};

const METHOD_SCHEMAS: PhonMethodSchemas = {
  argsRoot: 1n,
  argsSchemaClosure: "010203",
  okRoot: 2n,
  responseRoot: 3n,
  responseSchemaClosure: "040506",
  channels: [{ index: 0, direction: "tx", elementRoot: 9n }],
};

const ECHO_METHOD_KEY = `0x${SESSION_ECHO_METHOD_ID.toString(16).padStart(16, "0")}`;
const ECHO_METHOD_SCHEMAS = sessionEchoMethods[ECHO_METHOD_KEY]!;
const ECHO_METHOD: MethodDescriptor = {
  name: "echo",
  id: SESSION_ECHO_METHOD_ID,
};
const CHANNEL_DISCOVERY_METHOD_ID = 0x1234n;
const CHANNEL_DISCOVERY_METHOD_KEY = `0x${CHANNEL_DISCOVERY_METHOD_ID.toString(16).padStart(16, "0")}`;
const CHANNEL_DISCOVERY_METHOD: MethodDescriptor = {
  name: "channels",
  id: CHANNEL_DISCOVERY_METHOD_ID,
};
const U32_ROOT = ECHO_METHOD_SCHEMAS.okRoot;
const CHANNEL_DISCOVERY_SCHEMAS: PhonMethodSchemas = {
  argsRoot: 4n,
  argsSchemaClosure: "",
  okRoot: U32_ROOT,
  responseRoot: ECHO_METHOD_SCHEMAS.responseRoot,
  responseSchemaClosure: ECHO_METHOD_SCHEMAS.responseSchemaClosure,
  channels: [
    { index: 2, direction: "tx", elementRoot: U32_ROOT },
    { index: 0, direction: "rx", elementRoot: U32_ROOT },
  ],
};

describe("Driver channel schema exchange", () => {
  // r[verify rpc.unknown-method]
  it("responds with unknown method without closing the connection", async () => {
    const sent: Array<{ requestId: bigint; payload: Uint8Array }> = [];
    let dispatchCount = 0;
    let closeCount = 0;
    const driver = new Driver(
      {
        currentEpoch: () => 0,
        sendResponse: async (requestId: bigint, payload: Uint8Array) => {
          sent.push({ requestId, payload });
        },
        close: () => {
          closeCount += 1;
          throw new Error("unknown method must be a call-level response");
        },
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: { [ECHO_METHOD_KEY]: ECHO_METHOD_SCHEMAS },
          registry: sessionEchoRegistry,
          methods: new Map([[ECHO_METHOD.id, ECHO_METHOD]]),
        }),
        dispatch: async () => {
          dispatchCount += 1;
        },
      },
    ) as unknown as {
      handleCall(call: {
        requestId: bigint;
        methodId: bigint;
        args: Uint8Array;
        channels: bigint[];
        metadata: ReturnType<typeof emptyMetadata>;
        laneEpoch: number;
      }): Promise<void>;
    };

    await driver.handleCall({
      requestId: 12n,
      methodId: 0xdead_beefn,
      args: Uint8Array.of(0xff),
      channels: [99n],
      metadata: emptyMetadata(),
      laneEpoch: 0,
    });

    expect(dispatchCount).toBe(0);
    expect(closeCount).toBe(0);
    expect(sent).toHaveLength(1);
    expect(sent[0].requestId).toBe(12n);
    expect(
      decodeTyped(
        sent[0].payload,
        ECHO_METHOD_SCHEMAS.responseRoot,
        ECHO_METHOD_SCHEMAS.responseRoot,
        sessionEchoRegistry,
      ),
    ).toEqual({
      ok: false,
      error: {
        tag: "UnknownMethod",
      },
    });
  });

  // r[verify rpc.pipelining]
  it("allows a later request to reply while an earlier request is still pending", async () => {
    const sent: Array<{ requestId: bigint; payload: Uint8Array }> = [];
    let releaseFirst!: () => void;
    const firstMayFinish = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    let secondReplied!: () => void;
    const secondReplySent = new Promise<void>((resolve) => {
      secondReplied = resolve;
    });
    const incomingCalls = [
      {
        requestId: 1n,
        methodId: ECHO_METHOD.id,
        args: new Uint8Array(),
        channels: [],
        metadata: emptyMetadata(),
        laneEpoch: 0,
      },
      {
        requestId: 2n,
        methodId: ECHO_METHOD.id,
        args: new Uint8Array(),
        channels: [],
        metadata: emptyMetadata(),
        laneEpoch: 0,
      },
      null,
    ];
    let dispatchCount = 0;
    const driver = new Driver(
      {
        currentEpoch: () => 0,
        getSchemaSendTracker: () => new SchemaSendTracker(),
        getSchemaTracker: () => ({
          requireReceived() {},
        }),
        nextIncomingCall: async () => incomingCalls.shift() ?? null,
        nextIncomingCancel: () => new Promise<bigint | null>(() => {}),
        sendResponse: async (requestId: bigint, payload: Uint8Array) => {
          sent.push({ requestId, payload });
          if (requestId === 2n) {
            secondReplied();
          }
        },
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: { [ECHO_METHOD_KEY]: ECHO_METHOD_SCHEMAS },
          registry: sessionEchoRegistry,
          methods: new Map([[ECHO_METHOD.id, ECHO_METHOD]]),
        }),
        dispatch: async (_context, _method, _args, call) => {
          dispatchCount += 1;
          if (dispatchCount === 1) {
            await firstMayFinish;
            call.reply(1);
          } else {
            call.reply(2);
          }
        },
      },
    );

    const run = driver.run();
    await Promise.race([
      secondReplySent,
      new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error("second request was blocked by first request")), 1_000);
      }),
    ]);

    expect(sent.map((response) => response.requestId)).toEqual([2n]);

    releaseFirst();
    await run;

    expect(sent.map((response) => response.requestId)).toEqual([2n, 1n]);
    expect(
      decodeTyped(
        sent[0].payload,
        ECHO_METHOD_SCHEMAS.responseRoot,
        ECHO_METHOD_SCHEMAS.responseRoot,
        sessionEchoRegistry,
      ),
    ).toEqual({ ok: true, value: 2 });
  });

  // r[verify rpc]
  // r[verify rpc.channel.discovery]
  it("resolves callee channel handles from decoded wire indexes", async () => {
    const registry = new ChannelRegistry();
    const sentChannelData: Array<{ channelId: bigint; payload: Uint8Array }> = [];
    const sentResponses: bigint[] = [];
    let ordinaryArg: unknown;
    let rxChannelId: bigint | undefined;
    let txChannelId: bigint | undefined;
    let received: unknown;
    let dispatchError: unknown;
    const driver = new Driver(
      {
        id: 0n,
        currentEpoch: () => 0,
        localSettings: { initial_channel_credit: 4 },
        peerSettings: { initial_channel_credit: 4 },
        getChannelRegistry: () => registry,
        getSchemaSendTracker: () => new SchemaSendTracker(),
        getSchemaTracker: () => ({
          requireReceived() {},
          buildDecoder() {
            return () => [
              Uint8Array.of(1, 0, 0, 0),
              "ordinary",
              Uint8Array.of(0, 0, 0, 0),
            ];
          },
          buildAuxiliaryDecoder() {
            return undefined;
          },
        }),
        sendChannelData: async (channelId: bigint, payload: Uint8Array) => {
          sentChannelData.push({ channelId, payload });
        },
        sendResponse: async (requestId: bigint) => {
          sentResponses.push(requestId);
        },
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: { [CHANNEL_DISCOVERY_METHOD_KEY]: CHANNEL_DISCOVERY_SCHEMAS },
          registry: sessionEchoRegistry,
          methods: new Map([[CHANNEL_DISCOVERY_METHOD.id, CHANNEL_DISCOVERY_METHOD]]),
        }),
        dispatch: async (_context, _method, args, call) => {
          try {
            const rx = args[0] as { channelId: bigint; recv(): Promise<unknown> };
            const tx = args[2] as { channelId: bigint; send(value: unknown): Promise<void> };
            ordinaryArg = args[1];
            rxChannelId = rx.channelId;
            txChannelId = tx.channelId;

            registry.routeData(43n, Uint8Array.of(5, 0, 0, 0));
            received = await rx.recv();
            await tx.send(6);
            call.reply(7);
          } catch (error) {
            dispatchError = error;
            throw error;
          }
        },
      },
    ) as unknown as {
      handleCall(call: {
        requestId: bigint;
        methodId: bigint;
        args: Uint8Array;
        channels: bigint[];
        metadata: ReturnType<typeof emptyMetadata>;
        laneEpoch: number;
      }): Promise<void>;
    };

    await driver.handleCall({
      requestId: 33n,
      methodId: CHANNEL_DISCOVERY_METHOD.id,
      args: Uint8Array.of(0),
      channels: [41n, 43n],
      metadata: emptyMetadata(),
      laneEpoch: 0,
    });

    expect(ordinaryArg).toBe("ordinary");
    expect(rxChannelId).toBe(43n);
    expect(txChannelId).toBe(41n);
    expect(dispatchError).toBeUndefined();
    expect(received).toBe(5);
    expect(sentChannelData).toEqual([
      { channelId: 41n, payload: Uint8Array.of(6, 0, 0, 0) },
    ]);
    expect(sentResponses).toEqual([33n]);
  });

  // r[verify schema.errors.call-level]
  // r[verify schema.errors.call-level.callee]
  it("responds with invalid payload when callee args decode fails", async () => {
    const sent: Array<{ requestId: bigint; payload: Uint8Array; schemas: number[] }> = [];
    const schemaSendTracker = new SchemaSendTracker();
    let dispatchCount = 0;
    const driver = new Driver(
      {
        currentEpoch: () => 0,
        getSchemaSendTracker: () => schemaSendTracker,
        getSchemaTracker: () => ({
          requireReceived() {},
          buildDecoder() {
            return () => {
              throw new SchemaCompatibilityError("args decode plan failed");
            };
          },
        }),
        sendResponse: async (
          requestId: bigint,
          payload: Uint8Array,
          _metadata: unknown,
          _channels: bigint[],
          schemas: number[],
        ) => {
          sent.push({ requestId, payload, schemas });
        },
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: { [ECHO_METHOD_KEY]: ECHO_METHOD_SCHEMAS },
          registry: sessionEchoRegistry,
          methods: new Map([[ECHO_METHOD.id, ECHO_METHOD]]),
        }),
        dispatch: async () => {
          dispatchCount += 1;
        },
      },
    ) as unknown as {
      handleCall(call: {
        requestId: bigint;
        methodId: bigint;
        args: Uint8Array;
        channels: bigint[];
        metadata: ReturnType<typeof emptyMetadata>;
        laneEpoch: number;
      }): Promise<void>;
    };

    await driver.handleCall({
      requestId: 9n,
      methodId: ECHO_METHOD.id,
      args: Uint8Array.of(1),
      channels: [],
      metadata: emptyMetadata(),
      laneEpoch: 0,
    });

    expect(dispatchCount).toBe(0);
    expect(sent).toHaveLength(1);
    expect(sent[0].requestId).toBe(9n);
    expect(sent[0].schemas).toEqual(
      Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.responseSchemaClosure)),
    );
    expect(
      decodeTyped(
        sent[0].payload,
        ECHO_METHOD_SCHEMAS.responseRoot,
        ECHO_METHOD_SCHEMAS.responseRoot,
        sessionEchoRegistry,
      ),
    ).toEqual({
      ok: false,
      error: {
        tag: "InvalidPayload",
        value: "Schema compatibility error: args decode plan failed",
      },
    });
  });

  // r[verify schema.exchange.callee]
  it("advertises response schemas with the first callee response", async () => {
    const sent: Array<{ requestId: bigint; schemas: number[] }> = [];
    const schemaSendTracker = new SchemaSendTracker();
    const driver = new Driver(
      {
        currentEpoch: () => 0,
        getSchemaSendTracker: () => schemaSendTracker,
        getSchemaTracker: () => ({
          requireReceived() {},
        }),
        sendResponse: async (
          requestId: bigint,
          _payload: Uint8Array,
          _metadata: unknown,
          _channels: bigint[],
          schemas: number[],
        ) => {
          sent.push({ requestId, schemas });
        },
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: { [ECHO_METHOD_KEY]: ECHO_METHOD_SCHEMAS },
          registry: sessionEchoRegistry,
          methods: new Map([[ECHO_METHOD.id, ECHO_METHOD]]),
        }),
        dispatch: async (_context, _method, _args, call) => {
          call.reply(123);
        },
      },
    ) as unknown as {
      handleCall(call: {
        requestId: bigint;
        methodId: bigint;
        args: Uint8Array;
        channels: bigint[];
        metadata: ReturnType<typeof emptyMetadata>;
        laneEpoch: number;
      }): Promise<void>;
    };

    await driver.handleCall({
      requestId: 9n,
      methodId: ECHO_METHOD.id,
      args: new Uint8Array(),
      channels: [],
      metadata: emptyMetadata(),
      laneEpoch: 0,
    });

    expect(sent).toHaveLength(1);
    expect(sent[0]).toEqual({
      requestId: 9n,
      schemas: Array.from(hexToBytes(ECHO_METHOD_SCHEMAS.responseSchemaClosure)),
    });
  });

  // r[verify schema.exchange.channels.tx-args]
  it("advertises args schemas before the first server-written channel item", () => {
    const sent: TaskMessage[] = [];
    const driver = new Driver(
      {
        getSchemaSendTracker: () => new SchemaSendTracker(),
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: {},
          registry: {} as never,
          methods: new Map(),
        }),
        dispatch: async () => {},
      },
    ) as unknown as {
      argsSchemaAdvertisingTaskSender(
        method: MethodDescriptor,
        methodSchemas: PhonMethodSchemas,
        taskSender: (message: TaskMessage) => void,
      ): (message: TaskMessage) => void;
    };
    const sender = driver.argsSchemaAdvertisingTaskSender(METHOD, METHOD_SCHEMAS, (message) => {
      sent.push(message);
    });

    sender({ kind: "data", channelId: 11n, payload: Uint8Array.of(1) });
    sender({ kind: "data", channelId: 11n, payload: Uint8Array.of(2) });

    expect(sent.map((message) => message.kind)).toEqual(["schema", "data", "data"]);
    expect(sent[0]).toMatchObject({
      kind: "schema",
      methodId: 77n,
      direction: "args",
      schemas: Uint8Array.of(1, 2, 3),
    });
  });

  // r[verify schema.exchange.channels.rx-args]
  it("decodes server Rx channel items through the caller auxiliary root", () => {
    const seen: Array<[bigint, string, string, bigint]> = [];
    const tracker = {
      buildAuxiliaryDecoder(
        methodId: bigint,
        direction: "args" | "response",
        role: string,
        readerRoot: bigint,
      ) {
        seen.push([methodId, direction, role, readerRoot]);
        return (bytes: Uint8Array) => `rx:${bytes[0]}`;
      },
    } as unknown as SchemaTracker;
    const driver = new Driver(
      {
        getSchemaTracker: () => tracker,
      } as never,
      {
        getDescriptor: () => ({
          service_name: "Test",
          send_schemas: {},
          registry: {} as never,
          methods: new Map(),
        }),
        dispatch: async () => {},
      },
    ) as unknown as {
      channelElementDeserializer(
        method: MethodDescriptor,
        channel: PhonChannelMeta,
        registry: Registry,
      ): (bytes: Uint8Array) => unknown;
    };

    const decoder = driver.channelElementDeserializer(
      METHOD,
      { index: 1, direction: "rx", elementRoot: 456n },
      {} as Registry,
    );

    expect(decoder(Uint8Array.of(8))).toBe("rx:8");
    expect(seen).toEqual([[77n, "args", "channel.arg.1.rx.element", 456n]]);
  });
});
