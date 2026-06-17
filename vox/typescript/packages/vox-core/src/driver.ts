import {
  emptyMetadata,
} from "@bearcove/vox-wire";
import { encodeTyped, decodeTyped } from "@bearcove/phon-engine";
import type { Registry } from "@bearcove/phon-schema";
import {
  type MethodDescriptor,
  type VoxCall,
  type ServiceDescriptor,
  type TaskMessage,
  type TaskSender,
  createServerTx,
  createServerRx,
  DEFAULT_INITIAL_CREDIT,
} from "./channeling/index.ts";
import type { ServiceSendSchemas } from "./channeling/descriptor.ts";
import type { PhonChannelMeta, PhonMethodSchemas } from "./schema_tracker.ts";
import { SchemaCompatibilityError } from "./schema_tracker.ts";
import { Extensions } from "./middleware.ts";
import { RequestContext } from "./request_context.ts";
import { type ServerCallOutcome, type ServerMiddleware } from "./server_middleware.ts";
import type { IncomingCall, Lane } from "./connection.ts";
import { voxLogger } from "./logger.ts";

export interface Dispatcher {
  // r[impl rpc.service]
  // r[impl rpc.service.methods]
  // r[impl service-macro.is-source-of-truth]
  getDescriptor(): ServiceDescriptor;
  // r[impl rpc.handler]
  dispatch(
    context: RequestContext,
    method: MethodDescriptor,
    args: unknown[],
    call: VoxCall,
  ): Promise<void>;
}

/** The `send_schemas` map key for a method id (matches codegen `0x{:016x}`). */
function methodKey(id: bigint): string {
  return `0x${id.toString(16).padStart(16, "0")}`;
}

/** Read a little-endian `u32` from a 4-byte phon-compact scalar. */
function readU32LE(bytes: Uint8Array): number {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, true);
}

function channelElementRole(meta: PhonChannelMeta): string {
  return `channel.arg.${meta.index}.${meta.direction}.element`;
}

class VoxCallImpl implements VoxCall {
  private replied = false;

  private readonly method: MethodDescriptor;
  private readonly requestId: bigint;
  private readonly lane: Lane;
  private readonly laneEpoch: number;
  private readonly taskSender: TaskSender;
  private readonly schemaSendTracker: import("./schema_tracker.ts").SchemaSendTracker;
  private readonly methodSchemas: PhonMethodSchemas;
  private readonly registry: Registry;

  constructor(
    method: MethodDescriptor,
    requestId: bigint,
    lane: Lane,
    laneEpoch: number,
    taskSender: TaskSender,
    schemaSendTracker: import("./schema_tracker.ts").SchemaSendTracker,
    methodSchemas: PhonMethodSchemas,
    registry: Registry,
  ) {
    this.method = method;
    this.requestId = requestId;
    this.lane = lane;
    this.laneEpoch = laneEpoch;
    this.taskSender = taskSender;
    this.schemaSendTracker = schemaSendTracker;
    this.methodSchemas = methodSchemas;
    this.registry = registry;
  }

  didReply(): boolean {
    return this.replied;
  }

  reply(value: unknown): void {
    if (this.replied) {
      return;
    }
    this.replied = true;
    // A void handler returns `undefined`; phon's unit Value is `null`. Coerce so a
    // `Result<(), E>` Ok payload encodes (`??` keeps falsy values like `0`/`false`).
    const payload = this.encodeResponse({ tag: "Ok", value: value ?? null });
    this.sendPayload(payload);
  }

  // r[impl rpc.fallible]
  // r[impl rpc.fallible.vox-error]
  replyErr(error: unknown): void {
    if (this.replied) {
      return;
    }
    this.replied = true;
    const payload = this.encodeResponse({ tag: "Err", value: { tag: "User", value: error } });
    this.sendPayload(payload);
  }

  // r[impl rpc.error.scope]
  // r[impl rpc.fallible]
  // r[impl rpc.fallible.vox-error]
  replyInternalError(message = "Invalid payload"): void {
    if (this.replied) {
      return;
    }
    this.replied = true;
    const payload = this.encodeResponse({
      tag: "Err",
      value: { tag: "InvalidPayload", value: message },
    });
    this.sendPayload(payload);
  }

  /**
   * Encode a `Result<T, VoxError<E>>` response payload as phon bytes against the
   * method's `responseRoot`. The `{ tag, value }` shape mirrors the Rust
   * `RequestResponse.ret`.
   */
  private encodeResponse(result: {
    tag: "Ok" | "Err";
    value: unknown;
  }): Uint8Array {
    return encodeTyped(result as never, this.methodSchemas.responseRoot, this.registry);
  }

  /**
   * The phon schema-closure bytes to advertise for this method's response
   * binding, or undefined when already sent on this connection
   * (`r[schema.exchange.idempotent]`).
   */
  // r[impl schema.exchange.callee]
  private prepareResponseSchemas(): Uint8Array | undefined {
    const nums = this.schemaSendTracker.prepareSchemas(
      this.method.id,
      "response",
      this.methodSchemas.responseSchemaClosure,
    );
    return nums.length > 0 ? new Uint8Array(nums) : undefined;
  }

  private sendPayload(payload: Uint8Array): void {
    if (this.lane.currentEpoch() !== this.laneEpoch) {
      return;
    }
    this.taskSender({
      kind: "response",
      requestId: this.requestId,
      payload,
      schemas: this.prepareResponseSchemas(),
    });
  }
}

export class Driver {
  private readonly lane: Lane;
  private readonly dispatcher: Dispatcher;
  private readonly middlewares: ServerMiddleware[];
  private readonly taskQueue: TaskMessage[] = [];
  private inFlight = new Set<Promise<void>>();
  private wakeupResolve: (() => void) | null = null;

  static new(
    lane: Lane,
    dispatcher: Dispatcher,
    middlewares: ServerMiddleware[] = [],
  ): Driver {
    return new Driver(lane, dispatcher, middlewares);
  }

  constructor(
    lane: Lane,
    dispatcher: Dispatcher,
    middlewares: ServerMiddleware[] = [],
  ) {
    this.lane = lane;
    this.dispatcher = dispatcher;
    this.middlewares = middlewares;
  }

  withMiddleware(middleware: ServerMiddleware): Driver {
    return new Driver(this.lane, this.dispatcher, [...this.middlewares, middleware]);
  }

  async run(): Promise<void> {
    // r[impl rpc]
    // r[impl rpc.service]
    // r[impl rpc.handler]
    // r[impl lane.service]
    // r[impl rpc.pipelining]
    // r[impl rpc.connection-setup]
    let pendingIncoming: Promise<IncomingCall | null> | null = null;
    let pendingCancel: Promise<bigint | null> | null = null;

    while (true) {
      await this.flushTaskQueue();

      if (!pendingIncoming) {
        pendingIncoming = this.lane.nextIncomingCall();
      }
      if (!pendingCancel) {
        pendingCancel = this.lane.nextIncomingCancel();
      }

      const wakeup = new Promise<"wakeup">((resolve) => {
        this.wakeupResolve = () => resolve("wakeup");
      });

      const race = await Promise.race([
        pendingIncoming.then((call) => ({ kind: "incoming" as const, call })),
        pendingCancel.then((requestId) => ({ kind: "cancel" as const, requestId })),
        wakeup.then((kind) => ({ kind })),
      ]);

      if (race.kind === "wakeup") {
        continue;
      }

      if (race.kind === "cancel") {
        pendingCancel = null;
        if (race.requestId !== null) {
          this.handleCancel(race.requestId);
        }
        continue;
      }

      pendingIncoming = null;
      if (!race.call) {
        break;
      }

      const task = this.handleCall(race.call).finally(() => {
        this.inFlight.delete(task);
        this.signalWakeup();
      });
      this.inFlight.add(task);
    }

    await Promise.allSettled([...this.inFlight]);
    await this.flushTaskQueue();
  }

  private signalWakeup(): void {
    const wakeup = this.wakeupResolve;
    this.wakeupResolve = null;
    wakeup?.();
  }

  private async flushTaskQueue(): Promise<void> {
    while (this.taskQueue.length > 0) {
      const message = this.taskQueue.shift()!;
      switch (message.kind) {
        case "data":
          await this.lane.sendChannelData(message.channelId, message.payload).catch((error) => {
            voxLogger()?.error("[vox:driver] failed to send channel data", error);
          });
          break;
        case "close":
          await this.lane.sendChannelClose(message.channelId).catch((error) => {
            voxLogger()?.error("[vox:driver] failed to send channel close", error);
          });
          break;
        case "grantCredit":
          await this.lane.sendChannelCredit(message.channelId, message.additional).catch((error) => {
            voxLogger()?.error("[vox:driver] failed to grant channel credit", error);
          });
          break;
        case "schema":
          await this.lane
            .sendSchemas(message.methodId, message.direction, message.schemas)
            .catch((error) => {
              voxLogger()?.error("[vox:driver] failed to send schema message", error);
            });
          break;
        case "response":
          await this.lane
            .sendResponse(
              message.requestId,
              message.payload,
              emptyMetadata(),
              [],
              message.schemas ? Array.from(message.schemas) : [],
            )
            .catch((error) => {
              voxLogger()?.error("[vox:driver] failed to send response", error);
            });
          break;
      }
    }
  }

  private async handleCall(incoming: IncomingCall): Promise<void> {
    // r[impl rpc.service]
    // r[impl rpc.service.methods]
    // r[impl rpc.handler]
    // r[impl rpc.unknown-method]
    const descriptor = this.dispatcher.getDescriptor();
    const method = descriptor.methods.get(incoming.methodId);
    voxLogger()?.debug(`[vox:driver] handleCall: methodId=${incoming.methodId} method=${method?.name ?? "UNKNOWN"}`);
    if (!method) {
      voxLogger()?.debug(`[vox:driver] unknown method, sending error response`);
      await this.lane.sendResponse(incoming.requestId, encodeUnknownMethod(descriptor));
      return;
    }

    const context = new RequestContext(
      descriptor.service_name,
      method,
      incoming.metadata,
      new Extensions(),
    );

    const taskSender: TaskSender = (message) => {
      if (this.lane.currentEpoch() !== incoming.laneEpoch) {
        return;
      }
      this.taskQueue.push(message);
      this.signalWakeup();
    };

    const methodSchemas = descriptor.send_schemas[methodKey(method.id)];
    if (!methodSchemas) {
      voxLogger()?.error(`[vox:driver] no phon schemas for method ${method.id}`);
      await this.lane.sendResponse(incoming.requestId, encodeInvalidPayload(descriptor));
      return;
    }

    const call = new VoxCallImpl(
      method,
      incoming.requestId,
      this.lane,
      incoming.laneEpoch,
      taskSender,
      this.lane.getSchemaSendTracker(),
      methodSchemas,
      descriptor.registry,
    );

    let outcome: ServerCallOutcome = { kind: "dropped" };

    try {
      await this.runPreHooks(context);
      // r[impl schema.errors.call-level]
      // r[impl schema.errors.call-level.callee]
      const args = this.decodeArgs(
        descriptor,
        method,
        incoming,
        taskSender,
      );
      voxLogger()?.debug(`[vox:driver] dispatching ${method.name} with ${args.length} args`);
      await this.dispatcher.dispatch(context, method, args, call);
      voxLogger()?.debug(`[vox:driver] dispatch complete for ${method.name}, didReply=${call.didReply()}`);
      outcome = call.didReply() ? { kind: "replied" } : { kind: "dropped" };
      if (!call.didReply()) {
        call.replyInternalError();
      }
    } catch (error) {
      voxLogger()?.error(`[vox:driver] dispatch error for ${method.name}:`, error);
      if (!call.didReply()) {
        call.replyInternalError(error instanceof Error ? error.message : String(error));
      }
      outcome = { kind: "failed", error };
    }

    try {
      await this.runPostHooks(context, outcome);
    } finally {
      await this.flushTaskQueue();
    }
  }

  private handleCancel(_requestId: bigint): void {
  }

  private argsSchemaAdvertisingTaskSender(
    method: MethodDescriptor,
    methodSchemas: PhonMethodSchemas,
    taskSender: TaskSender,
  ): TaskSender {
    // r[impl schema.exchange.channels.tx-args]
    let advertised = false;
    return (message) => {
      if (!advertised && message.kind === "data") {
        advertised = true;
        const schemas = this.lane.getSchemaSendTracker().prepareSchemas(
          method.id,
          "args",
          methodSchemas.argsSchemaClosure,
        );
        if (schemas.length > 0) {
          taskSender({
            kind: "schema",
            methodId: method.id,
            direction: "args",
            schemas: new Uint8Array(schemas),
          });
        }
      }
      taskSender(message);
    };
  }

  private channelElementDeserializer(
    method: MethodDescriptor,
    channel: PhonChannelMeta,
    registry: Registry,
  ): (bytes: Uint8Array) => unknown {
    // r[impl schema.exchange.channels.rx-args]
    const role = channelElementRole(channel);
    return (bytes) => {
      const decoder = this.lane.getSchemaTracker().buildAuxiliaryDecoder(
        method.id,
        "args",
        role,
        channel.elementRoot,
        registry,
      );
      if (decoder) {
        return decoder(bytes) as unknown;
      }
      return decodeTyped(bytes, channel.elementRoot, channel.elementRoot, registry);
    };
  }

  private decodeArgs(
    descriptor: ServiceDescriptor,
    method: MethodDescriptor,
    incoming: IncomingCall,
    taskSender: TaskSender,
  ): unknown[] {
    // r[impl rpc.channel.binding]
    // r[impl rpc.channel.binding.callee-args.rx]
    // r[impl rpc.channel.binding.callee-args.tx]
    const ms = descriptor.send_schemas[methodKey(method.id)];
    if (!ms) {
      throw new Error(`no phon schemas for method ${method.id}`);
    }
    const registry = descriptor.registry;

    // Decode the args tuple using the peer's writer closure (recorded by
    // the connection in the `schemas:` field) against our `argsRoot` reader. The connection
    // receive path requires a binding even for 0-arg methods.
    // r[impl schema.exchange.required]
    // r[impl schema.errors.call-level.callee]
    this.lane.getSchemaTracker().requireReceived(method.id, "args");
    let values: unknown[] = [];
    if (incoming.args.length > 0) {
      const decoder = this.lane.getSchemaTracker().buildDecoder(
        method.id,
        "args",
        ms.argsRoot,
        registry,
      );
      if (!decoder) {
        throw new SchemaCompatibilityError(`missing args schema binding for method ${method.id}`);
      }
      values = decoder(incoming.args) as unknown[];
    }

    if (ms.channels.length === 0) {
      return values;
    }

    // Bind each server-side `Tx`/`Rx` from `RequestCall.channels`. The decoded
    // arg at a channel position is the 4-byte LE wire index into that list
    // (`r[rpc.channel.payload-encoding]`); resolve it to a `ChannelId` and replace
    // the slot with a runtime handle whose per-item codec is keyed on the element.
    const channelRegistry = this.lane.getChannelRegistry();
    const creditOut = this.lane.peerSettings.initial_channel_credit ?? DEFAULT_INITIAL_CREDIT;
    const creditIn = this.lane.localSettings.initial_channel_credit ?? DEFAULT_INITIAL_CREDIT;
    const serverTxTaskSender = ms.channels.some((ch) => ch.direction === "tx")
      ? this.argsSchemaAdvertisingTaskSender(method, ms, taskSender)
      : taskSender;
    // r[impl rpc.channel.discovery]
    for (const ch of ms.channels) {
      const wireIndex = readU32LE(values[ch.index] as Uint8Array);
      const channelId = incoming.channels[wireIndex];
      if (channelId === undefined) {
        throw new Error(`channel wire index ${wireIndex} out of range (${incoming.channels.length})`);
      }
      channelRegistry.rememberContext(channelId, {
        laneId: this.lane.id,
        requestId: incoming.requestId,
        service: descriptor.service_name,
        method: method.name,
        channelDirection: ch.direction,
        side: "server",
      });
      if (ch.direction === "tx") {
        // The handler holds a `Tx` and SENDS to the caller.
        values[ch.index] = createServerTx(
          channelId,
          serverTxTaskSender,
          channelRegistry,
          creditOut,
          (value: unknown) => encodeTyped(value as never, ch.elementRoot, registry),
        );
      } else {
        // The handler holds an `Rx` and RECEIVES from the caller.
        const receiver = channelRegistry.registerIncoming(channelId, creditIn);
        values[ch.index] = createServerRx(
          channelId,
          receiver,
          this.channelElementDeserializer(method, ch, registry),
        );
      }
    }

    return values;
  }

  private async runPreHooks(context: RequestContext): Promise<void> {
    for (const middleware of this.middlewares) {
      await middleware.pre?.(context);
    }
  }

  private async runPostHooks(
    context: RequestContext,
    outcome: ServerCallOutcome,
  ): Promise<void> {
    for (let i = this.middlewares.length - 1; i >= 0; i--) {
      await this.middlewares[i]?.post?.(context, outcome);
    }
  }
}

// Protocol-error responses are `Result<T, VoxError<E>>::Err(VoxError::…)`. The
// `Err` payload (UnknownMethod / Cancelled / Indeterminate / InvalidPayload) is
// independent of the method's `T`/`E`, so any method's `responseRoot` encodes it;
// the caller decodes against its own response root (no schema is advertised).
function encodeVoxError(
  descriptor: ServiceDescriptor,
  err: { tag: string; value?: unknown },
): Uint8Array {
  for (const ms of Object.values(descriptor.send_schemas)) {
    return encodeTyped({ tag: "Err", value: err } as never, ms.responseRoot, descriptor.registry);
  }
  throw new Error("service has no methods to derive a response root");
}

function encodeUnknownMethod(descriptor: ServiceDescriptor): Uint8Array {
  return encodeVoxError(descriptor, { tag: "UnknownMethod" });
}

function encodeInvalidPayload(descriptor: ServiceDescriptor): Uint8Array {
  return encodeVoxError(descriptor, { tag: "InvalidPayload", value: "invalid payload" });
}
