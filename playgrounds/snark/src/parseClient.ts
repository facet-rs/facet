// Main-thread client for the persistent parse worker. One worker holds the prepared
// session; we talk to it with a tiny id-keyed RPC so parsing never blocks the UI thread.
import type { ParseWorkerRequest, ParseWorkerResponse } from "./parseWorker";

export type RunParseInput = Omit<ParseWorkerRequest, "id">;
export type RunParseResult = {
  response: string;
  prepared: boolean;
  vix: string | null;
  vixMachine: string | null;
};

let worker: Worker | null = null;
let nextId = 1;
const pending = new Map<number, { resolve: (value: RunParseResult) => void; reject: (error: Error) => void }>();

function ensureWorker(): Worker {
  if (worker) {
    return worker;
  }
  worker = new Worker(new URL("./parseWorker.ts", import.meta.url), { type: "module" });
  worker.onmessage = (event: MessageEvent<ParseWorkerResponse>) => {
    const data = event.data;
    const entry = pending.get(data.id);
    if (!entry) {
      return;
    }
    pending.delete(data.id);
    if (data.ok) {
      entry.resolve({
        response: data.response,
        prepared: data.prepared,
        vix: data.vix,
        vixMachine: data.vixMachine,
      });
    } else {
      entry.reject(new Error(data.error));
    }
  };
  worker.onerror = (event) => {
    const error = new Error(event.message || "parse worker crashed");
    for (const entry of pending.values()) {
      entry.reject(error);
    }
    pending.clear();
    worker?.terminate();
    worker = null;
  };
  return worker;
}

export function runParse(payload: RunParseInput): Promise<RunParseResult> {
  const id = nextId++;
  const target = ensureWorker();
  return new Promise<RunParseResult>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    target.postMessage({ id, ...payload } satisfies ParseWorkerRequest);
  });
}
