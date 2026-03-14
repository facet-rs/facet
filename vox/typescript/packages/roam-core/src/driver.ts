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
import { metadataOperationId } from "./retry.ts";
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

interface OperationSignature {
  methodId: bigint;
  args: Uint8Array;
}

interface StoredOperation {
  signature: OperationSignature;
  retry: MethodDescriptor["retry"];
}

interface LiveOperation {
  kind: "live";
  stored: StoredOperation;
  ownerRequestId: bigint;
  waiters: bigint[];
}

interface ReleasedOperation {
  kind: "released";
  stored: StoredOperation;
}

interface IndeterminateOperation {
  kind: "indeterminate";
  stored: StoredOperation;
}

interface SealedOperation {
  kind: "sealed";
  stored: StoredOperation;
  payload: Uint8Array;
}

type OperationState =
  | LiveOperation
  | ReleasedOperation
  | IndeterminateOperation
  | SealedOperation;

type OperationAdmit =
  | { kind: "start" }
  | { kind: "attached" }
  | { kind: "replay"; payload: Uint8Array }
  | { kind: "conflict" }
  | { kind: "indeterminate" };

type OperationCancel =
  | { kind: "none" }
  | { kind: "detach" }
  | { kind: "release"; ownerRequestId: bigint; waiters: bigint[] };

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

function sameSignature(
  signature: OperationSignature,
  methodId: bigint,
  args: Uint8Array,
): boolean {
  return signature.methodId === methodId && sameBytes(signature.args, args);
}

class OperationRegistry {
  private readonly states = new Map<bigint, OperationState>();
  private readonly requestToOperation = new Map<bigint, bigint>();

  admit(
    operationId: bigint,
    methodId: bigint,
    args: Uint8Array,
    retry: MethodDescriptor["retry"],
    requestId: bigint,
  ): OperationAdmit {
    const signature: OperationSignature = {
      methodId,
      args: args.slice(),
    };
    const existing = this.states.get(operationId);
    if (!existing) {
      this.requestToOperation.set(requestId, operationId);
      this.states.set(operationId, {
        kind: "live",
        stored: { signature, retry },
        ownerRequestId: requestId,
        waiters: [requestId],
      });
      return { kind: "start" };
    }

    switch (existing.kind) {
      case "live":
        if (!sameSignature(existing.stored.signature, methodId, args)) {
          return { kind: "conflict" };
        }
        existing.waiters.push(requestId);
        this.requestToOperation.set(requestId, operationId);
        return { kind: "attached" };
      case "sealed":
        if (!sameSignature(existing.stored.signature, methodId, args)) {
          return { kind: "conflict" };
        }
        return { kind: "replay", payload: existing.payload.slice() };
      case "released":
      case "indeterminate":
        if (!sameSignature(existing.stored.signature, methodId, args) || !existing.stored.retry.idem) {
          return sameSignature(existing.stored.signature, methodId, args)
            ? { kind: "indeterminate" }
            : { kind: "conflict" };
        }
        this.requestToOperation.set(requestId, operationId);
        this.states.set(operationId, {
          kind: "live",
          stored: { signature, retry: existing.stored.retry },
          ownerRequestId: requestId,
          waiters: [requestId],
        });
        return { kind: "start" };
    }
  }

  seal(operationId: bigint, ownerRequestId: bigint, payload: Uint8Array): bigint[] {
    const existing = this.states.get(operationId);
    if (!existing || existing.kind !== "live" || existing.ownerRequestId !== ownerRequestId) {
      return [];
    }
    for (const waiter of existing.waiters) {
      this.requestToOperation.delete(waiter);
    }
    this.states.set(operationId, {
      kind: "sealed",
      stored: existing.stored,
      payload: payload.slice(),
    });
    return [...existing.waiters];
  }

  failWithoutReply(operationId: bigint, ownerRequestId: bigint): bigint[] {
    const existing = this.states.get(operationId);
    if (!existing || existing.kind !== "live" || existing.ownerRequestId !== ownerRequestId) {
      return [];
    }
    for (const waiter of existing.waiters) {
      this.requestToOperation.delete(waiter);
    }
    this.states.set(operationId, existing.stored.retry.persist
      ? { kind: "indeterminate", stored: existing.stored }
      : { kind: "released", stored: existing.stored });
    return [...existing.waiters];
  }

  cancel(requestId: bigint): OperationCancel {
    const operationId = this.requestToOperation.get(requestId);
    if (operationId === undefined) {
      return { kind: "none" };
    }
    const existing = this.states.get(operationId);
    if (!existing || existing.kind !== "live") {
      this.requestToOperation.delete(requestId);
      return { kind: "none" };
    }
    if (existing.stored.retry.persist) {
      if (existing.ownerRequestId === requestId) {
        return { kind: "none" };
      }
      existing.waiters = existing.waiters.filter((candidate) => candidate !== requestId);
      this.requestToOperation.delete(requestId);
      return { kind: "detach" };
    }
    for (const waiter of existing.waiters) {
      this.requestToOperation.delete(waiter);
    }
    this.states.set(operationId, { kind: "released", stored: existing.stored });
    return {
      kind: "release",
      ownerRequestId: existing.ownerRequestId,
      waiters: [...existing.waiters],
    };
  }
}

class RoamCallImpl implements RoamCall {
  private replied = false;

  constructor(
    private readonly method: MethodDescriptor,
    private readonly requestId: bigint,
    private readonly taskSender: TaskSender,
    private readonly operations: OperationRegistry,
    private readonly operationId: bigint | undefined,
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
    this.sendPayload(payload);
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
    this.sendPayload(payload);
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
    this.sendPayload(payload);
  }

  private sendPayload(payload: Uint8Array): void {
    if (this.operationId === undefined) {
      this.taskSender({ kind: "response", requestId: this.requestId, payload });
      return;
    }
    const waiters = this.operations.seal(this.operationId, this.requestId, payload);
    for (const waiter of waiters) {
      this.taskSender({ kind: "response", requestId: waiter, payload: payload.slice() });
    }
  }
}

export class Driver {
  private readonly middlewares: ServerMiddleware[];
  private readonly taskQueue: TaskMessage[] = [];
  private readonly operations = new OperationRegistry();
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
    // r[impl rpc.session-setup]
    let pendingIncoming: Promise<IncomingCall | null> | null = null;
    let pendingCancel: Promise<bigint | null> | null = null;

    while (true) {
      await this.flushTaskQueue();

      if (!pendingIncoming) {
        pendingIncoming = this.connection.nextIncomingCall();
      }
      if (!pendingCancel) {
        pendingCancel = this.connection.nextIncomingCancel();
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
          await this.connection.sendChannelData(message.channelId, message.payload).catch(() => {});
          break;
        case "close":
          await this.connection.sendChannelClose(message.channelId).catch(() => {});
          break;
        case "grantCredit":
          await this.connection.sendChannelCredit(message.channelId, message.additional).catch(() => {});
          break;
        case "response":
          await this.connection.sendResponse(message.requestId, message.payload).catch(() => {});
          break;
      }
    }
  }

  private async handleCall(incoming: IncomingCall): Promise<void> {
    // r[impl rpc.unknown-method]
    // r[impl rpc.response.one-per-request]
    const descriptor = this.dispatcher.getDescriptor();
    const method = descriptor.methods.find((candidate) => candidate.id === incoming.methodId);
    if (!method) {
      await this.connection.sendResponse(incoming.requestId, encodeUnknownMethod());
      return;
    }

    const operationId = metadataOperationId(incoming.metadata);
    if (operationId !== undefined) {
      const admit = this.operations.admit(
        operationId,
        incoming.methodId,
        incoming.args,
        method.retry,
        incoming.requestId,
      );
      switch (admit.kind) {
        case "attached":
          return;
        case "replay":
          await this.connection.sendResponse(incoming.requestId, admit.payload);
          return;
        case "conflict":
          await this.connection.sendResponse(incoming.requestId, encodeInvalidPayload());
          return;
        case "indeterminate":
          await this.connection.sendResponse(incoming.requestId, encodeIndeterminate());
          return;
        case "start":
          break;
      }
    }

    const context = new RequestContext(
      descriptor.service_name,
      method,
      incoming.metadata,
      new Extensions(),
    );
    const failClosedOnDrop = incoming.channels.length > 0 && !method.retry.idem;

    const taskSender: TaskSender = (message) => {
      this.taskQueue.push(message);
      this.signalWakeup();
    };

    const call = new RoamCallImpl(
      method,
      incoming.requestId,
      taskSender,
      this.operations,
      operationId,
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
        if (operationId !== undefined) {
          const waiters = this.operations.failWithoutReply(operationId, incoming.requestId);
          for (const waiter of waiters) {
            taskSender({
              kind: "response",
              requestId: waiter,
              payload: method.retry.persist || failClosedOnDrop ? encodeIndeterminate() : encodeCancelled(),
            });
          }
        } else if (method.retry.persist) {
          this.taskQueue.push({
            kind: "response",
            requestId: incoming.requestId,
            payload: encodeIndeterminate(),
          });
        } else {
          call.replyInternalError();
        }
      }
    } catch (error) {
      if (!call.didReply()) {
        if (operationId !== undefined) {
          const waiters = this.operations.failWithoutReply(operationId, incoming.requestId);
          for (const waiter of waiters) {
            taskSender({
              kind: "response",
              requestId: waiter,
              payload: method.retry.persist || failClosedOnDrop ? encodeIndeterminate() : encodeCancelled(),
            });
          }
        } else if (method.retry.persist) {
          this.taskQueue.push({
            kind: "response",
            requestId: incoming.requestId,
            payload: encodeIndeterminate(),
          });
        } else {
          call.replyInternalError();
        }
      }
      outcome = { kind: "failed", error };
    }

    try {
      await this.runPostHooks(context, outcome);
    } finally {
      await this.flushTaskQueue();
    }
  }

  private handleCancel(requestId: bigint): void {
    const cancel = this.operations.cancel(requestId);
    switch (cancel.kind) {
      case "none":
        return;
      case "detach":
        return;
      case "release":
        for (const waiter of cancel.waiters) {
          this.taskQueue.push({ kind: "response", requestId: waiter, payload: encodeCancelled() });
        }
        this.signalWakeup();
        return;
    }
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

function encodeInvalidPayload(): Uint8Array {
  return new Uint8Array([0x01, 0x02]);
}

function encodeCancelled(): Uint8Array {
  return new Uint8Array([0x01, 0x03]);
}

function encodeIndeterminate(): Uint8Array {
  return new Uint8Array([0x01, 0x04]);
}
