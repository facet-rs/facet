import { decodeWithSchema, encodeWithSchema } from "@bearcove/roam-postcard";
import {
  RpcError,
  RpcErrorCode,
} from "@bearcove/roam-wire";
import {
  DEFAULT_INITIAL_CREDIT,
  type MethodDescriptor,
  type RoamCall,
  type ServiceDescriptor,
  createServerRx,
  createServerTx,
  type TaskMessage,
  type TaskSender,
} from "./channeling/index.ts";
import { Extensions } from "./middleware.ts";
import { RequestContext } from "./request_context.ts";
import { type ServerCallOutcome, type ServerMiddleware } from "./server_middleware.ts";
import type { ConnectionHandle, IncomingCall } from "./session.ts";

export interface Dispatcher {
  getDescriptor(): ServiceDescriptor;
  dispatch(
    context: RequestContext,
    method: MethodDescriptor,
    args: unknown[],
    call: RoamCall,
  ): Promise<void>;
}

class RoamCallImpl implements RoamCall {
  private replied = false;

  constructor(
    private readonly method: MethodDescriptor,
    private readonly requestId: bigint,
    private readonly taskSender: TaskSender,
    private readonly schemaRegistry?: ServiceDescriptor["schema_registry"],
  ) {}

  didReply(): boolean {
    return this.replied;
  }

  reply(value: unknown): void {
    if (this.replied) {
      return;
    }
    this.replied = true;
    const payload = encodeWithSchema(
      { tag: "Ok", value },
      this.method.result,
      this.schemaRegistry,
    );
    this.taskSender({ kind: "response", requestId: this.requestId, payload });
  }

  replyErr(error: unknown): void {
    if (this.replied) {
      return;
    }
    this.replied = true;
    const payload = encodeWithSchema(
      { tag: "Err", value: { tag: "User", value: error } },
      this.method.result,
      this.schemaRegistry,
    );
    this.taskSender({ kind: "response", requestId: this.requestId, payload });
  }

  replyInternalError(): void {
    if (this.replied) {
      return;
    }
    this.replied = true;
    const payload = encodeWithSchema(
      { tag: "Err", value: { tag: "InvalidPayload" } },
      this.method.result,
      this.schemaRegistry,
    );
    this.taskSender({ kind: "response", requestId: this.requestId, payload });
  }
}

export class Driver {
  private readonly middlewares: ServerMiddleware[];
  private readonly taskQueue: TaskMessage[] = [];
  private inFlight = new Set<Promise<void>>();
  private wakeupResolve: (() => void) | null = null;

  constructor(
    private readonly connection: ConnectionHandle,
    private readonly dispatcher: Dispatcher,
    middlewares: ServerMiddleware[] = [],
  ) {
    this.middlewares = middlewares;
  }

  withMiddleware(middleware: ServerMiddleware): Driver {
    return new Driver(this.connection, this.dispatcher, [...this.middlewares, middleware]);
  }

  async run(): Promise<void> {
    let pendingIncoming: Promise<IncomingCall | null> | null = null;

    while (true) {
      await this.flushTaskQueue();

      if (!pendingIncoming) {
        pendingIncoming = this.connection.nextIncomingCall();
      }

      const wakeup = new Promise<"wakeup">((resolve) => {
        this.wakeupResolve = () => resolve("wakeup");
      });

      const race = await Promise.race([
        pendingIncoming.then((call) => ({ kind: "incoming" as const, call })),
        wakeup.then((kind) => ({ kind })),
      ]);

      if (race.kind === "wakeup") {
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
          await this.connection.sendChannelData(message.channelId, message.payload);
          break;
        case "close":
          await this.connection.sendChannelClose(message.channelId);
          break;
        case "grantCredit":
          await this.connection.sendChannelCredit(message.channelId, message.additional);
          break;
        case "response":
          await this.connection.sendResponse(message.requestId, message.payload);
          break;
      }
    }
  }

  private async handleCall(incoming: IncomingCall): Promise<void> {
    const descriptor = this.dispatcher.getDescriptor();
    const method = descriptor.methods.find((candidate) => candidate.id === incoming.methodId);
    if (!method) {
      await this.connection.sendResponse(incoming.requestId, encodeUnknownMethod());
      return;
    }

    const context = new RequestContext(
      descriptor.service_name,
      method,
      incoming.metadata,
      new Extensions(),
    );

    const taskSender: TaskSender = (message) => {
      this.taskQueue.push(message);
      this.signalWakeup();
    };

    const call = new RoamCallImpl(
      method,
      incoming.requestId,
      taskSender,
      descriptor.schema_registry,
    );

    let outcome: ServerCallOutcome = { kind: "dropped" };

    try {
      await this.runPreHooks(context);
      const args = this.decodeArgs(
        descriptor,
        method,
        incoming,
        taskSender,
      );
      await this.dispatcher.dispatch(context, method, args, call);
      outcome = call.didReply() ? { kind: "replied" } : { kind: "dropped" };
      if (!call.didReply()) {
        call.replyInternalError();
      }
    } catch (error) {
      if (!call.didReply()) {
        call.replyInternalError();
      }
      outcome = { kind: "failed", error };
    }

    try {
      await this.runPostHooks(context, outcome);
    } finally {
      await this.flushTaskQueue();
    }
  }

  private decodeArgs(
    descriptor: ServiceDescriptor,
    method: MethodDescriptor,
    incoming: IncomingCall,
    taskSender: TaskSender,
  ): unknown[] {
    const decoded = decodeWithSchema(
      incoming.args,
      0,
      method.args,
      descriptor.schema_registry,
    );
    if (decoded.next !== incoming.args.length) {
      throw new RpcError(RpcErrorCode.INVALID_PAYLOAD);
    }

    let channelIndex = 0;
    return (decoded.value as unknown[]).map((raw, argIndex) => {
      const argSchema = method.args.elements[argIndex];
      if (argSchema.kind === "tx") {
        const channelId = incoming.channels[channelIndex++];
        return createServerTx(
          channelId,
          taskSender,
          this.connection.getChannelRegistry(),
          argSchema.initial_credit ?? DEFAULT_INITIAL_CREDIT,
          (value: unknown) => encodeWithSchema(value, argSchema.element, descriptor.schema_registry),
        );
      }
      if (argSchema.kind === "rx") {
        const channelId = incoming.channels[channelIndex++];
        const receiver = this.connection.getChannelRegistry().registerIncoming(
          channelId,
          argSchema.initial_credit ?? DEFAULT_INITIAL_CREDIT,
          (additional) => {
            taskSender({ kind: "grantCredit", channelId, additional });
          },
        );
        return createServerRx(channelId, receiver, (bytes: Uint8Array) =>
          decodeWithSchema(bytes, 0, argSchema.element, descriptor.schema_registry).value,
        );
      }
      return raw;
    });
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

function encodeUnknownMethod(): Uint8Array {
  return new Uint8Array([0x01, 0x01]);
}
