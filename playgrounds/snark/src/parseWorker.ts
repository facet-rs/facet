// Runs the snark-wasm parse session OFF the main thread. Building a parse table for a
// heavy grammar (e.g. gingembre) takes seconds and is synchronous; doing it on the main
// thread freezes the whole UI, which is brutal on mobile. The worker holds the prepared
// session so the main thread only ever posts a message and renders the result.
import init, { SnarkPlaygroundSession } from "@bearcove/snark-wasm";

export type ParseWorkerRequest = {
  id: number;
  key: string;
  /** Present only when the grammar bundle changed and the session must be (re)prepared. */
  files: { path: string; text: string }[] | null;
  input: string;
  runCorpus: boolean;
  edit: unknown | null;
  useReparse: boolean;
};

export type ParseWorkerResponse =
  | { id: number; ok: true; response: string; prepared: boolean }
  | { id: number; ok: false; error: string };

const ready = init();
let session: SnarkPlaygroundSession | null = null;
let sessionKey: string | null = null;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function post(response: ParseWorkerResponse) {
  (self as unknown as Worker).postMessage(response);
}

self.onmessage = async (event: MessageEvent<ParseWorkerRequest>) => {
  const { id, key, files, input, runCorpus, edit, useReparse } = event.data;
  try {
    const initStart = performance.now();
    await ready;
    const initMs = performance.now() - initStart;
    if (initMs > 1) console.log(`[snark load] wasm instantiate: ${initMs.toFixed(0)} ms`);

    // (Re)prepare the session when the grammar bundle changed — but only if we're not
    // already prepared for this exact key. The key is content-hashed (grammar + files),
    // so an unchanged language yields the same key; redundant prepare requests (React
    // StrictMode double-invoke, effect churn during the multi-second prepare window,
    // repeated typing/sample switches) must not rebuild the tables again.
    if (files && sessionKey !== key) {
      if (session) {
        session.free();
        session = null;
        sessionKey = null;
      }
      const prepStart = performance.now();
      session = new SnarkPlaygroundSession(JSON.stringify({ files }));
      console.log(
        `[snark load] session prepare (tables + plan): ${(performance.now() - prepStart).toFixed(0)} ms`,
      );
      sessionKey = key;
    }

    if (!session || sessionKey !== key) {
      post({ id, ok: false, error: "parse session is not prepared for this grammar" });
      return;
    }

    const request = JSON.stringify({
      input,
      run_corpus: runCorpus,
      edit: useReparse ? edit : null,
    });
    const response = useReparse ? session.reparse(request) : session.parse(request);
    post({ id, ok: true, response, prepared: true });
  } catch (error) {
    post({ id, ok: false, error: errorMessage(error) });
  }
};
