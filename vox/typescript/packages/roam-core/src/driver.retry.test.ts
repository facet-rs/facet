import { describe, expect, it } from "vitest";
import type { Message } from "@bearcove/roam-wire";
import { BareConduit } from "./conduit.ts";
import { Driver, type Dispatcher } from "./driver.ts";
import { Extensions } from "./middleware.ts";
import { RequestContext } from "./request_context.ts";
import { Session } from "./session.ts";
import { ClientMetadata } from "./metadata.ts";
import { OPERATION_ID_METADATA_KEY } from "./retry.ts";
import type { MethodDescriptor, ServiceDescriptor } from "./channeling/index.ts";

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
  args: { kind: "tuple", elements: [{ kind: "u32" }] },
  result: {
    kind: "enum",
    variants: [
      { name: "Ok", fields: { kind: "u32" } },
      {
        name: "Err",
        fields: {
          kind: "enum",
          variants: [
            { name: "User", fields: null },
            { name: "UnknownMethod", fields: null },
            { name: "InvalidPayload", fields: null },
            { name: "Cancelled", fields: null },
          ],
        },
      },
    ],
  },
};

const DESCRIPTOR: ServiceDescriptor = {
  service_name: "Test",
  methods: [METHOD],
};

describe("retry operation identity", () => {
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

    const [serverSession, clientSession] = await Promise.all([
      Session.establishAcceptor(serverConduit),
      Session.establishInitiator(clientConduit),
    ]);
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

    const [serverSession, clientSession] = await Promise.all([
      Session.establishAcceptor(serverConduit),
      Session.establishInitiator(clientConduit),
    ]);
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
    });
    const second = caller.call({
      method: "Test.echo",
      args: { value: 11 },
      descriptor: METHOD,
      metadata: new ClientMetadata(),
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
    });
    expect(replayed).toBe(11);
    expect(runs).toBe(1);

    clientLink.close();
    serverLink.close();
    await Promise.allSettled([serverRun, clientRun]);
  });
});
