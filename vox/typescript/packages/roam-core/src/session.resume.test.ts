import { describe, expect, it } from "vitest";
import { RpcErrorCode } from "@bearcove/roam-wire";
import { BareConduit } from "./conduit.ts";
import { Driver, type Dispatcher } from "./driver.ts";
import { RequestContext } from "./request_context.ts";
import { Session, SessionError, type SessionHandle } from "./session.ts";
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
  conduit: BareConduit,
): Promise<void> {
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

function makeMethod(retry: MethodDescriptor["retry"]): MethodDescriptor {
  return {
    name: "echo",
    id: 1n,
    retry,
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
              { name: "Indeterminate", fields: null },
            ],
          },
        },
      ],
    },
  };
}

const PERSIST_METHOD = makeMethod({ persist: true, idem: false });
const IDEM_METHOD = makeMethod({ persist: false, idem: true });
const VOLATILE_METHOD = makeMethod({ persist: false, idem: false });

function descriptorFor(method: MethodDescriptor): ServiceDescriptor {
  return {
    service_name: "Test",
    methods: [method],
  };
}

describe("session resumption", () => {
  it("keeps a pending call alive across manual resume on a new conduit", async () => {
    const [clientLink1, serverLink1] = memoryLinkPair();
    const clientConduit1 = new BareConduit(clientLink1);
    const serverConduit1 = new BareConduit(serverLink1);
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

    const [serverSession, clientSession] = await withTimeout(
      Promise.all([
        Session.establishAcceptor(serverConduit1, { resumable: true }),
        Session.establishInitiator(clientConduit1, { resumable: true }),
      ]),
      "initial session establishment",
    );
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 55 },
      descriptor: PERSIST_METHOD,
    });

    await withTimeout(started.promise, "handler start");

    clientLink1.close();
    serverLink1.close();

    const [clientLink2, serverLink2] = memoryLinkPair();
    const clientConduit2 = new BareConduit(clientLink2);
    const serverConduit2 = new BareConduit(serverLink2);

    await withTimeout(
      Promise.all([
        resumeWhenReady(serverSession.handle(), serverConduit2),
        resumeWhenReady(clientSession.handle(), clientConduit2),
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
    const clientConduit1 = new BareConduit(clientLink1);
    const serverConduit1 = new BareConduit(serverLink1);
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

    const [serverSession, clientSession] = await withTimeout(
      Promise.all([
        Session.establishAcceptor(serverConduit1, { resumable: true }),
        Session.establishInitiator(clientConduit1, { resumable: true }),
      ]),
      "initial session establishment",
    );
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 77 },
      descriptor: IDEM_METHOD,
    });

    await withTimeout(firstStarted.promise, "first handler start");
    clientLink1.close();
    serverLink1.close();
    dropFirst.resolve();

    const [clientLink2, serverLink2] = memoryLinkPair();
    const clientConduit2 = new BareConduit(clientLink2);
    const serverConduit2 = new BareConduit(serverLink2);

    await withTimeout(
      Promise.all([
        resumeWhenReady(serverSession.handle(), serverConduit2),
        resumeWhenReady(clientSession.handle(), clientConduit2),
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
    const clientConduit1 = new BareConduit(clientLink1);
    const serverConduit1 = new BareConduit(serverLink1);
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

    const [serverSession, clientSession] = await withTimeout(
      Promise.all([
        Session.establishAcceptor(serverConduit1, { resumable: true }),
        Session.establishInitiator(clientConduit1, { resumable: true }),
      ]),
      "initial session establishment",
    );
    const serverDriver = new Driver(serverSession.rootConnection(), dispatcher);
    const serverRun = serverDriver.run();

    const call = clientSession.rootConnection().caller().call({
      method: "Test.echo",
      args: { value: 88 },
      descriptor: VOLATILE_METHOD,
    });

    await withTimeout(firstStarted.promise, "first handler start");
    clientLink1.close();
    serverLink1.close();
    dropFirst.resolve();

    const [clientLink2, serverLink2] = memoryLinkPair();
    const clientConduit2 = new BareConduit(clientLink2);
    const serverConduit2 = new BareConduit(serverLink2);

    await withTimeout(
      Promise.all([
        resumeWhenReady(serverSession.handle(), serverConduit2),
        resumeWhenReady(clientSession.handle(), clientConduit2),
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
});
