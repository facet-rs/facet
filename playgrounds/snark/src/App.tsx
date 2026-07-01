import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { runParse } from "./parseClient";
import { runBenchmark, BenchPanel, type BenchReport } from "./benchmark";
import { SourceEditor, type SourceEdit } from "./editor";
import { captureClass } from "./highlight";
import { defaultVendoredRootId, vendoredFiles } from "./bundled";
import {
  discoverGrammarRoots,
  filesWithGrammarJson,
  grammarRootForId,
  normalizeBundleFiles,
  preferredGrammarRootId,
  preferredSampleForGrammarRootId,
  projectedFilesForGrammarRootId,
  sourceExamplesForGrammarRootId,
  sortedSampleFiles,
  sortedFiles,
  normalizePath,
  type DslBundleFile,
  type ProjectedDslBundleFile,
} from "./treeSitterDsl";

type BundleFile = DslBundleFile;

type SampleFile = ProjectedDslBundleFile;

type Diagnostic = {
  stage: string;
  message: string;
  primary_span: DiagnosticSpan | null;
};

type DiagnosticSpan = {
  start_byte: number;
  end_byte: number;
  start_row: number;
  start_column: number;
  end_row: number;
  end_column: number;
};

type ParseOutput = {
  sexp: string;
  tree: ResolvedTreeOutput | null;
  accepted_count: number;
  failure_count: number;
  max_live_versions: number;
  trace_event_count: number;
  tree_event_count: number;
  reuse_node_count: number;
  accepted_tree_event_count: number;
  accepted_error_count: number;
  accepted_missing_count: number;
};

type PlanOutput = {
  fully_visible: boolean;
  parser_fully_visible: boolean;
  lexer_fully_visible: boolean;
  neutral_weavy_only: boolean;
  stencils_needed: boolean;
  neutral_weavy_op_count: number;
  snark_intrinsic_count: number;
  snark_stencils: PlanStencilOutput[];
  lowering_barriers: PlanBarrierOutput[];
};

type PlanStencilOutput = {
  descriptor: string;
  domain: string;
  lowering: string;
  family: string;
  execution: string;
  state: string[];
  effect: PlanStencilEffectOutput;
  count: number;
};

type PlanStencilEffectOutput = {
  ordering: string;
  resource_count: number;
  typed_memory_count: number;
  may_fail: boolean;
  may_allocate: boolean;
  calls_user_code: boolean;
  opaque: boolean;
};

type PlanBarrierOutput = {
  kind: string;
  count: number;
};

type ResolvedTreeOutput = {
  kind: string;
  field: string | null;
  text: string | null;
  start_byte: number;
  end_byte: number;
  start_row: number;
  start_column: number;
  end_row: number;
  end_column: number;
  named: boolean;
  visible: boolean;
  extra: boolean;
  children: ResolvedTreeOutput[];
};

type HighlightOutput = {
  capture_name: string;
  text: string;
  start_byte: number;
  end_byte: number;
  start_row: number;
  start_column: number;
  end_row: number;
  end_column: number;
};

type InjectionOutput = {
  language: string;
  combined: boolean;
  include_children: boolean;
  text: string;
  start_byte: number;
  end_byte: number;
  start_row: number;
  start_column: number;
  end_row: number;
  end_column: number;
};

type LayerOutput = {
  language: string;
  combined: boolean;
  ranges: LayerSourceRange[];
  input: string;
  parse: ParseOutput | null;
  highlights: HighlightOutput[];
  injections: InjectionOutput[];
  layers: LayerOutput[];
  diagnostics: Diagnostic[];
};

type LayerSourceRange = {
  text: string;
  start_byte: number;
  end_byte: number;
  start_row: number;
  start_column: number;
  end_row: number;
  end_column: number;
};

type CorpusOutput = {
  path: string;
  case_name: string;
  passed: boolean;
  input: string;
  expected: string;
  actual: string | null;
  error: string | null;
};

type TestSummary = {
  requested: boolean;
  corpus_passed: number;
  corpus_failed: number;
  highlight_assertions_passed: number;
  highlight_assertions_failed: number;
  highlight_fixture_errors: number;
};

type HighlightTestOutput = {
  path: string;
  passed: boolean;
  input: string;
  assertion_count: number;
  passed_count: number;
  failed_count: number;
  assertions: HighlightAssertionOutput[];
  error: string | null;
};

type HighlightAssertionOutput = {
  capture_name: string;
  negative: boolean;
  passed: boolean;
  row: number;
  column: number;
  length: number;
  observed_captures: string[];
  message: string | null;
};

type PhaseTiming = { name: string; ms: number };

type Timings = { prepare: PhaseTiming[]; parse: PhaseTiming | null };

type PlaygroundResponse = {
  ok: boolean;
  language: string | null;
  diagnostics: Diagnostic[];
  bundle: {
    grammar_path: string | null;
    grammar_js_path: string | null;
    query_paths: string[];
    corpus_paths: string[];
    sample_paths: string[];
    generated_files_ignored: string[];
    scanner_paths: string[];
    active_scanner: string | null;
  };
  plan: PlanOutput | null;
  parse: ParseOutput | null;
  highlights: HighlightOutput[];
  injections: InjectionOutput[];
  layers: LayerOutput[];
  corpus: CorpusOutput[];
  highlight_tests: HighlightTestOutput[];
  tests: TestSummary;
  timings: Timings;
};

const defaultFiles: BundleFile[] = vendoredFiles;
const defaultGrammarRoot = defaultVendoredRootId;
// One frame (~60fps). Leading-edge throttle interval for live re-parsing.
const PARSE_THROTTLE_MS = 16;
const defaultSample = preferredSampleForGrammarRootId(defaultFiles, defaultGrammarRoot);

type PendingSourceEdit = {
  oldInput: string;
  edit: SourceEdit;
};

type BundledTestSnapshot = Pick<PlaygroundResponse, "corpus" | "highlight_tests" | "tests"> & {
  key: string;
};

export function App() {
  const [files, setFiles] = useState<BundleFile[]>(defaultFiles);
  const [selectedGrammarRoot, setSelectedGrammarRoot] = useState(defaultGrammarRoot);
  const [selectedSamplePath, setSelectedSamplePath] = useState(defaultSample?.path ?? "");
  const [input, setInput] = useState(defaultSample?.text ?? "");
  const [result, setResult] = useState<PlaygroundResponse | null>(null);
  const [busyTask, setBusyTask] = useState<"parse" | "tests" | "bench" | null>(null);
  const [benchReport, setBenchReport] = useState<BenchReport | null>(null);
  const [benchProgress, setBenchProgress] = useState("");
  const parseRequestId = useRef(0);
  const autoTestedKeyRef = useRef<string | null>(null);
  const bundledTestSnapshotRef = useRef<BundledTestSnapshot | null>(null);
  // The prepared session lives in the parse worker; here we only track which grammar it's
  // prepared for and the last input it parsed (for incremental-reparse gating).
  const preparedKeyRef = useRef<string | null>(null);
  const baselineInputRef = useRef<string | null>(null);
  const pendingSourceEditRef = useRef<PendingSourceEdit | null>(null);
  // In-flight/last DSL emit keyed by session key, so repeated prepare-triggering
  // renders (StrictMode double-invoke, effect churn during the multi-second prepare
  // window) reuse one grammar.js -> grammar.json emit instead of respawning the DSL
  // worker each time.
  const grammarJsonCacheRef = useRef<{ key: string; promise: Promise<BundleFile[]> } | null>(null);
  // Leading-edge throttle: the parse itself is ~a few ms, so the first change runs
  // immediately and a burst (fast typing) coalesces to at most one run per frame.
  const lastParseAtRef = useRef(0);

  const grammarRoots = useMemo(() => discoverGrammarRoots(files), [files]);
  const activeGrammarRoot = useMemo(
    () => grammarRootForId(files, selectedGrammarRoot),
    [files, selectedGrammarRoot],
  );
  const activeGrammarRootId = activeGrammarRoot?.id ?? selectedGrammarRoot;
  const projectedFiles = useMemo(
    () => projectedFilesForGrammarRootId(files, activeGrammarRootId),
    [files, activeGrammarRootId],
  );
  const sourceInputs = useMemo(
    () => sourceExamplesForGrammarRootId(files, activeGrammarRootId),
    [files, activeGrammarRootId],
  );
  const visibleBundleFiles = useMemo(
    () => sortedRuntimeBundleFiles(projectedFiles),
    [projectedFiles],
  );
  const hasBundledTests = useMemo(
    () =>
      projectedFiles.some(
        (file) =>
          file.path.startsWith("test/corpus/") ||
          file.path.startsWith("test/highlight/") ||
          file.path.startsWith("test/highlights/"),
      ),
    [projectedFiles],
  );
  const busy = busyTask !== null;

  const handleRunBenchmark = async (): Promise<BenchReport> => {
    const grammar = activeGrammarRootId;
    const samples = projectedFiles
      .filter((file) => file.path.startsWith("samples/"))
      .map((file) => ({ name: file.path.replace(/^samples\//, ""), text: file.text }));
    if (!samples.length) {
      throw new Error(`grammar "${grammar}" has no samples to benchmark`);
    }
    setBusyTask("bench");
    setBenchProgress(`0/${samples.length}`);
    try {
      const key = sessionCacheKey(grammar, projectedFiles);
      const parse = async (text: string) => {
        const runnableFiles =
          preparedKeyRef.current !== key ? await filesWithGrammarJson(files, grammar) : null;
        const { response, prepared } = await runParse({
          key,
          files: runnableFiles,
          input: text,
          runCorpus: false,
          edit: null,
          useReparse: false,
        });
        if (prepared) preparedKeyRef.current = key;
        return JSON.parse(response) as PlaygroundResponse;
      };
      const report = await runBenchmark({
        grammar,
        samples,
        parse,
        onProgress: (done, total, name) => setBenchProgress(`${done}/${total} · ${name}`),
      });
      setBenchReport(report);
      window.__snarkBenchResult = report;
      return report;
    } finally {
      setBusyTask(null);
      setBenchProgress("");
    }
  };

  const benchHandlerRef = useRef(handleRunBenchmark);
  benchHandlerRef.current = handleRunBenchmark;
  useEffect(() => {
    window.__snarkRunBenchmark = () => benchHandlerRef.current();
    return () => {
      delete window.__snarkRunBenchmark;
    };
  }, []);

  const editorCaptures = useMemo(() => composedHighlights(result), [result]);
  const editorDiagnostic = useMemo(() => {
    const found = placedDiagnostic(result);
    return found?.primary_span
      ? { stage: found.stage, message: found.message, span: found.primary_span }
      : null;
  }, [result]);

  useEffect(() => {
    const requestId = parseRequestId.current + 1;
    parseRequestId.current = requestId;
    const key = sessionCacheKey(activeGrammarRootId, projectedFiles);
    const runBundledTests = hasBundledTests && autoTestedKeyRef.current !== key;
    // Only flash the busy indicator for genuinely slow work — a grammar (re)prepare or
    // a test run. A live reparse is a few ms; a spinner would just flicker.
    if (runBundledTests || preparedKeyRef.current !== key) {
      setBusyTask(runBundledTests ? "tests" : "parse");
    }

    const run = () => {
      lastParseAtRef.current = performance.now();
      void playgroundResponse(runBundledTests)
        .then((response) => {
          if (parseRequestId.current === requestId) {
            if (runBundledTests) {
              autoTestedKeyRef.current = key;
            }
            setResult(response);
          }
        })
        .finally(() => {
          if (parseRequestId.current === requestId) {
            setBusyTask(null);
          }
        });
    };

    // Leading edge: run now if it's been at least one frame since the last parse;
    // otherwise schedule the trailing run exactly one frame after it. A burst of
    // edits keeps clearing this timeout and collapses to a single run.
    const sinceLast = performance.now() - lastParseAtRef.current;
    const delay = Math.max(0, PARSE_THROTTLE_MS - sinceLast);
    if (delay === 0) {
      run();
      return;
    }
    const timeout = window.setTimeout(run, delay);
    return () => {
      window.clearTimeout(timeout);
    };
  }, [activeGrammarRootId, files, hasBundledTests, input, projectedFiles]);

  async function loadFiles(fileList: FileList | null) {
    if (!fileList || fileList.length === 0) {
      return;
    }
    const loaded = await Promise.all(
      Array.from(fileList).map(async (file) => ({
        path: rawBrowserPath(file),
        text: await file.text(),
      })),
    );
    const next = normalizeBundleFiles(loaded);
    const nextGrammarRoot = preferredGrammarRootId(next);
    const nextSample = preferredSampleForGrammarRootId(next, nextGrammarRoot);
    autoTestedKeyRef.current = null;
    bundledTestSnapshotRef.current = null;
    preparedKeyRef.current = null;
    baselineInputRef.current = null;
    pendingSourceEditRef.current = null;
    setFiles(next);
    setSelectedGrammarRoot(nextGrammarRoot);
    setSelectedSamplePath(nextSample?.path ?? "");
    setInput(nextSample?.text ?? "");
    setResult(null);
  }

  function updateSourceInput(nextInput: string, samplePath = "", edit: SourceEdit | null = null) {
    pendingSourceEditRef.current = edit ? { oldInput: input, edit } : null;
    setInput(nextInput);
    setSelectedSamplePath(samplePath);
    // On an incremental edit, keep the last result so the editor remaps its existing
    // highlight decorations through the change (CodeMirror does this for us) instead of
    // flashing unstyled until the next parse lands. Only drop highlights on a full
    // replace — sample/grammar switch or "Use input" — where the old spans are meaningless.
    if (!edit) {
      setResult(null);
    }
  }

  async function playgroundResponse(runBundledTests: boolean): Promise<PlaygroundResponse> {
    try {
      const key = sessionCacheKey(activeGrammarRootId, projectedFiles);
      const needPrepare = preparedKeyRef.current !== key;

      // Only emit grammar.js -> grammar.json (in the DSL worker) when the bundle
      // changed, and dedup concurrent/repeat emits for the same key via a cached
      // promise so we never respawn the DSL worker for a language we're already emitting.
      let runnableFiles: BundleFile[] | null = null;
      if (needPrepare) {
        if (grammarJsonCacheRef.current?.key !== key) {
          const emitStart = performance.now();
          const promise = filesWithGrammarJson(files, activeGrammarRootId).then((emitted) => {
            console.log(
              `[snark load] DSL emit grammar.js -> grammar.json: ${(performance.now() - emitStart).toFixed(0)} ms`,
            );
            return emitted;
          });
          grammarJsonCacheRef.current = { key, promise };
        }
        try {
          runnableFiles = await grammarJsonCacheRef.current.promise;
        } catch (error) {
          grammarJsonCacheRef.current = null; // allow a later attempt to retry the emit
          throw new PlaygroundRunError("grammar.js", errorMessage(error));
        }
      }

      const pendingEdit = pendingSourceEditRef.current;
      const useReparse =
        !runBundledTests &&
        !needPrepare &&
        pendingEdit !== null &&
        baselineInputRef.current === pendingEdit.oldInput &&
        pendingEdit.oldInput !== input;

      let result: { response: string; prepared: boolean };
      try {
        result = await runParse({
          key,
          files: runnableFiles,
          input,
          runCorpus: runBundledTests,
          edit: useReparse ? pendingEdit.edit : null,
          useReparse,
        });
      } catch (error) {
        // A worker/prepare failure: force a fresh prepare on the next run.
        preparedKeyRef.current = null;
        baselineInputRef.current = null;
        throw new PlaygroundRunError("snark", errorMessage(error));
      }

      preparedKeyRef.current = result.prepared ? key : null;
      const parsed = JSON.parse(result.response) as PlaygroundResponse;
      if (runBundledTests) {
        bundledTestSnapshotRef.current = {
          key,
          corpus: parsed.corpus,
          highlight_tests: parsed.highlight_tests,
          tests: parsed.tests,
        };
      } else if (!parsed.tests.requested && bundledTestSnapshotRef.current?.key === key) {
        parsed.corpus = bundledTestSnapshotRef.current.corpus;
        parsed.highlight_tests = bundledTestSnapshotRef.current.highlight_tests;
        parsed.tests = bundledTestSnapshotRef.current.tests;
      }
      if (parsed.parse) {
        baselineInputRef.current = input;
        pendingSourceEditRef.current = null;
      }
      return parsed;
    } catch (error) {
      const diagnostic =
        error instanceof PlaygroundRunError
          ? { stage: error.stage, message: error.message }
          : { stage: "playground", message: errorMessage(error) };
      return responseWithDiagnostic(diagnostic.stage, diagnostic.message, projectedFiles);
    }
  }

  async function runBundleTests() {
    const requestId = parseRequestId.current + 1;
    parseRequestId.current = requestId;
    setBusyTask("tests");
    const key = sessionCacheKey(activeGrammarRootId, projectedFiles);
    const response = await playgroundResponse(true);
    if (parseRequestId.current === requestId) {
      autoTestedKeyRef.current = key;
      setResult(response);
      setBusyTask(null);
    }
  }

  return (
    <main className="shell">
      <aside className="rail" aria-label="Bundle files">
        <header className="rail-head">
          <div className="brand">
            <span className="brand-mark" aria-hidden="true">
              ◢◣
            </span>
            <div className="brand-text">
              <h1>Snark</h1>
              <p>{result?.language ?? "mini_playground"}</p>
            </div>
          </div>
          <label className="upload">
            <input
              type="file"
              multiple
              {...({ webkitdirectory: "", directory: "" } as Record<string, string>)}
              onChange={(event) => void loadFiles(event.currentTarget.files)}
            />
            <span>Upload</span>
          </label>
        </header>

        <div className="file-list">
          {visibleBundleFiles.map((file) =>
            file.path.startsWith("samples/") ? (
              <button
                type="button"
                key={file.sourcePath}
                className={file.path === selectedSamplePath ? "file-row active" : "file-row"}
                title={file.sourcePath === file.path ? file.path : `${file.path} from ${file.sourcePath}`}
                onClick={() => updateSourceInput(file.text, file.path)}
              >
                <span className="file-name">{file.path}</span>
                <span className="file-size">{file.text.length.toLocaleString()}</span>
              </button>
            ) : (
              <div
                key={file.sourcePath}
                className="file-row static"
                title={file.sourcePath === file.path ? file.path : `${file.path} from ${file.sourcePath}`}
              >
                <span className="file-name">{file.path}</span>
                <span className="file-size">{file.text.length.toLocaleString()}</span>
              </div>
            ),
          )}
        </div>

        <BundleInventory result={result} files={projectedFiles} sourceInputs={sourceInputs} />
      </aside>

      <section className="workspace" aria-label="Source and results">
        <div className="toolbar">
          <div className="toolbar-selects">
            {grammarRoots.length > 1 ? (
              <select
                aria-label="Grammar root"
                className="select"
                value={activeGrammarRoot?.id ?? ""}
                onChange={(event) => {
                  const nextGrammarRoot = event.currentTarget.value;
                  const nextSample = preferredSampleForGrammarRootId(files, nextGrammarRoot);
                  autoTestedKeyRef.current = null;
                  bundledTestSnapshotRef.current = null;
                  preparedKeyRef.current = null;
                  baselineInputRef.current = null;
                  pendingSourceEditRef.current = null;
                  setSelectedGrammarRoot(nextGrammarRoot);
                  setSelectedSamplePath(nextSample?.path ?? "");
                  setInput(nextSample?.text ?? "");
                  setResult(null);
                }}
              >
                {grammarRoots.map((root) => (
                  <option key={root.id} value={root.id}>
                    {root.label}
                  </option>
                ))}
              </select>
            ) : null}
            {sourceInputs.length > 0 ? (
              <select
                aria-label="Source input"
                className="select"
                value={selectedSamplePath}
                onChange={(event) => {
                  const sample = sourceInputs.find((file) => file.path === event.currentTarget.value);
                  if (sample) {
                    updateSourceInput(sample.text, sample.path);
                  } else {
                    setSelectedSamplePath("");
                  }
                }}
              >
                <option value="">Source · {sourceInputs.length}</option>
                {sourceInputs.map((file) => (
                  <option key={file.sourcePath} value={file.path}>
                    {file.path}
                  </option>
                ))}
              </select>
            ) : null}
          </div>
          <div className="toolbar-end">
            <StatusPill result={result} busyTask={busyTask} />
            {hasBundledTests ? (
              <button type="button" className="btn" onClick={() => void runBundleTests()} disabled={busy}>
                {busyTask === "tests" ? "Running…" : "Run tests"}
              </button>
            ) : null}
          </div>
        </div>

        <SourceEditor
          input={input}
          captures={editorCaptures}
          diagnostic={editorDiagnostic}
          onChange={(value, edit) => updateSourceInput(value, "", edit)}
        />

        <div className="dock bench-dock">
          <BenchPanel
            report={benchReport}
            running={busyTask === "bench"}
            progress={benchProgress}
            onRun={() => void handleRunBenchmark()}
          />
        </div>

        <ResultsDock result={result} onUseInput={(value, sourcePath = "") => updateSourceInput(value, sourcePath)} />
      </section>
    </main>
  );
}

function StatusPill({
  result,
  busyTask,
}: {
  result: PlaygroundResponse | null;
  busyTask: "parse" | "tests" | "bench" | null;
}) {
  if (busyTask) {
    return (
      <span className="pill busy">
        <span className="dot" />
        {busyTask === "tests" ? "Running tests" : busyTask === "bench" ? "Benchmarking" : "Parsing"}
      </span>
    );
  }
  if (!result) {
    return (
      <span className="pill idle">
        <span className="dot" />
        Ready
      </span>
    );
  }
  if (!result.ok) {
    const recovered =
      result.parse && (result.parse.accepted_error_count > 0 || result.parse.accepted_missing_count > 0);
    return (
      <span className={recovered ? "pill warn" : "pill error"}>
        <span className="dot" />
        {recovered ? "Recovered with errors" : "Parse failed"}
      </span>
    );
  }
  const failures =
    result.tests.corpus_failed +
    result.tests.highlight_assertions_failed +
    result.tests.highlight_fixture_errors;
  if (failures > 0) {
    return (
      <span className="pill warn">
        <span className="dot" />
        {failures} test {failures === 1 ? "failure" : "failures"}
      </span>
    );
  }
  return (
    <span className="pill ok">
      <span className="dot" />
      {result.parse ? `Accepted ${result.parse.accepted_count}` : "Passed"}
    </span>
  );
}

// A collapsible panel whose body is mounted only while open — so a collapsed panel
// costs nothing to reconcile. `<details>` hides content with CSS but React still
// renders (and the browser still lays out) every node, which is what made switching
// pages re-render the whole parse tree / capture list on every parse.
function Panel({
  title,
  meta,
  defaultOpen = false,
  children,
}: {
  title: string;
  meta?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className={`panel ${open ? "open" : ""}`}>
      <button
        type="button"
        className="panel-summary"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="panel-title">{title}</span>
        {meta != null ? <span className="panel-meta">{meta}</span> : null}
      </button>
      {open ? <div className="panel-body">{children}</div> : null}
    </div>
  );
}

function ResultsDock({
  result,
  onUseInput,
}: {
  result: PlaygroundResponse | null;
  onUseInput: (value: string, sourcePath?: string) => void;
}) {
  const failure = result && !result.ok ? result : null;
  const sexp = result?.parse?.sexp ?? "";
  const captures = composedHighlights(result);
  const layers = result?.layers ?? [];
  const tests = result?.tests.requested ? result : null;
  const unplaced = result?.diagnostics.filter((diagnostic) => !diagnostic.primary_span) ?? [];
  const recovered =
    failure?.parse &&
    (failure.parse.accepted_error_count > 0 || failure.parse.accepted_missing_count > 0);

  const timingRows: Array<{ name: string; ms: number; kind: "prepare" | "parse" }> = [
    ...(result?.timings?.prepare ?? []).map((phase) => ({ ...phase, kind: "prepare" as const })),
    ...(result?.timings?.parse ? [{ ...result.timings.parse, kind: "parse" as const }] : []),
  ];
  const maxTimingMs = timingRows.reduce((max, row) => Math.max(max, row.ms), 0) || 1;

  return (
    <div className="dock">
      {failure ? (
        <div className="dock-failure">
          <strong>{recovered ? "Recovered with errors" : "Parse failed"}</strong>
          {unplaced.map((diagnostic, index) => (
            <div className="dock-failure-row" key={`${diagnostic.stage}-${index}`}>
              <span className="dock-failure-stage">{diagnostic.stage}</span>
              <code>{diagnostic.message}</code>
            </div>
          ))}
        </div>
      ) : null}

      {timingRows.length ? (
        <Panel
          title="Timings"
          defaultOpen
          meta={
            result?.timings?.parse
              ? `run parser ${result.timings.parse.ms.toFixed(2)} ms`
              : "prepare only"
          }
        >
          <div className="timing-list">
            {timingRows.map((row) => (
              <div className={`timing-row timing-${row.kind}`} key={`${row.kind}-${row.name}`}>
                <span className="timing-name">{row.name}</span>
                <span className="timing-track">
                  <span
                    className="timing-bar"
                    style={{ width: `${Math.max(2, (row.ms / maxTimingMs) * 100)}%` }}
                  />
                </span>
                <span className="timing-ms">{row.ms.toFixed(row.ms >= 1 ? 2 : 3)} ms</span>
              </div>
            ))}
          </div>
          <p className="timing-note">
            Prepare phases run once per grammar; “run parser” is live per input — watch it stay flat as input grows.
          </p>
        </Panel>
      ) : null}

      <Panel
        title="S-expression"
        meta={
          result?.parse ? (
            <>
              {result.parse.accepted_count} accepted · {result.parse.failure_count} failed
              {result.parse.reuse_node_count ? ` · ${result.parse.reuse_node_count} reused` : ""}
              {result.parse.accepted_error_count || result.parse.accepted_missing_count
                ? ` · ${result.parse.accepted_error_count} ERROR · ${result.parse.accepted_missing_count} MISSING`
                : ""}
            </>
          ) : undefined
        }
      >
        {sexp ? <pre className="sexp">{sexp}</pre> : <p className="empty">No parse tree.</p>}
      </Panel>

      <Panel title="Captures" meta={captures.length}>
        {captures.length ? (
          <div className="capture-list">
            {captures.map((capture, index) => (
              <div className="capture-row" key={`${capture.capture_name}-${capture.start_byte}-${index}`}>
                <span className={`capture-chip ${captureClass(capture.capture_name)}`}>
                  @{capture.capture_name}
                </span>
                <code>{capture.text}</code>
                <span className="capture-loc">
                  {capture.start_row}:{capture.start_column}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <p className="empty">No captures.</p>
        )}
      </Panel>

      {layers.length ? (
        <Panel title="Layers" meta={countLayers(layers)}>
          <LayerList layers={layers} />
        </Panel>
      ) : null}

      {tests ? (
        <Panel
          title="Tests"
          defaultOpen
          meta={
            <>
              {tests.tests.corpus_passed + tests.tests.highlight_assertions_passed} pass ·{" "}
              {tests.tests.corpus_failed +
                tests.tests.highlight_assertions_failed +
                tests.tests.highlight_fixture_errors}{" "}
              fail
            </>
          }
        >
            <div className="corpus-list">
              {tests.corpus.map((caseResult, index) => (
                <details className="case" key={`${caseResult.path}-${caseResult.case_name}-${index}`}>
                  <summary className={caseResult.passed ? "pass" : "fail"}>
                    {caseResult.case_name}
                    <button
                      type="button"
                      className="ghost"
                      onClick={(event) => {
                        event.preventDefault();
                        onUseInput(caseResult.input, `${caseResult.path}#${caseResult.case_name}`);
                      }}
                    >
                      Use input
                    </button>
                  </summary>
                  <div className="test-detail-grid">
                    {caseResult.error ? (
                      <section>
                        <h3>Error</h3>
                        <pre>{caseResult.error}</pre>
                      </section>
                    ) : null}
                    <section>
                      <h3>Expected</h3>
                      <pre>{caseResult.expected}</pre>
                    </section>
                    <section>
                      <h3>Actual</h3>
                      <pre>{caseResult.actual ?? ""}</pre>
                    </section>
                  </div>
                </details>
              ))}
              {tests.highlight_tests.map((fixture) => (
                <details className="case" key={fixture.path}>
                  <summary className={fixture.passed ? "pass" : "fail"}>
                    {fixture.path} ({fixture.passed_count}/{fixture.assertion_count})
                    <button
                      type="button"
                      className="ghost"
                      onClick={(event) => {
                        event.preventDefault();
                        onUseInput(fixture.input, fixture.path);
                      }}
                    >
                      Use fixture
                    </button>
                  </summary>
                  {fixture.error ? (
                    <pre>{fixture.error}</pre>
                  ) : (
                    <div className="assertion-list">
                      {fixture.assertions.map((assertion, index) => (
                        <div
                          className={assertion.passed ? "assertion-row pass" : "assertion-row fail"}
                          key={`${fixture.path}-${assertion.row}-${assertion.column}-${index}`}
                        >
                          <span>
                            {assertion.negative ? "!" : ""}@{assertion.capture_name}
                          </span>
                          <span className="capture-loc">
                            {assertion.row}:{assertion.column}
                          </span>
                          {assertion.message ? <code>{assertion.message}</code> : null}
                        </div>
                      ))}
                    </div>
                  )}
                </details>
              ))}
            </div>
        </Panel>
      ) : null}
    </div>
  );
}

function LayerList({ layers }: { layers: LayerOutput[] }) {
  return (
    <div className="layer-list">
      {layers.map((layer, index) => (
        <LayerNode layer={layer} key={`${layer.language}-${layer.ranges[0]?.start_byte ?? 0}-${index}`} />
      ))}
    </div>
  );
}

function LayerNode({ layer }: { layer: LayerOutput }) {
  const failed = layer.diagnostics.length > 0 || !layer.parse;
  return (
    <details className="layer-node" open={failed || layer.layers.length > 0}>
      <summary className={failed ? "fail" : "pass"}>
        <span className="layer-lang">{layer.language}</span>
        <span className="layer-meta">
          {layer.combined ? "combined" : "region"} · {layer.ranges.length} range
          {layer.ranges.length === 1 ? "" : "s"}
          {layer.parse
            ? ` · ${layer.parse.accepted_count} accepted · ${layer.parse.failure_count} failed`
            : " · no parse"}
        </span>
      </summary>
      <div className="layer-body">
        <div className="layer-ranges">
          {layer.ranges.map((range, index) => (
            <span className="capture-loc" key={`${range.start_byte}-${index}`}>
              {range.start_row}:{range.start_column}-{range.end_row}:{range.end_column}
            </span>
          ))}
        </div>
        {layer.diagnostics.length ? (
          <div className="layer-diagnostics">
            {layer.diagnostics.map((diagnostic, index) => (
              <div className="dock-failure-row" key={`${diagnostic.stage}-${index}`}>
                <span className="dock-failure-stage">{diagnostic.stage}</span>
                <code>{diagnostic.message}</code>
              </div>
            ))}
          </div>
        ) : null}
        {layer.highlights.length ? (
          <div className="capture-list">
            {layer.highlights.map((capture, index) => (
              <div className="capture-row" key={`${capture.capture_name}-${capture.start_byte}-${index}`}>
                <span className={`capture-chip ${captureClass(capture.capture_name)}`}>
                  @{capture.capture_name}
                </span>
                <code>{capture.text}</code>
                <span className="capture-loc">
                  {capture.start_row}:{capture.start_column}
                </span>
              </div>
            ))}
          </div>
        ) : null}
        {layer.parse?.sexp ? <pre className="sexp layer-sexp">{layer.parse.sexp}</pre> : null}
        {layer.layers.length ? <LayerList layers={layer.layers} /> : null}
      </div>
    </details>
  );
}

function countLayers(layers: LayerOutput[]): number {
  return layers.reduce((count, layer) => count + 1 + countLayers(layer.layers), 0);
}

function composedHighlights(result: PlaygroundResponse | null): HighlightOutput[] {
  if (!result) {
    return [];
  }
  return [
    ...result.highlights.map((highlight) => ({ ...highlight, priority: 0 })),
    ...layerHighlights(result.layers, 1),
  ];
}

function layerHighlights(layers: LayerOutput[], depth: number): HighlightOutput[] {
  return layers.flatMap((layer) => [
    ...layer.highlights.map((highlight) => ({ ...highlight, priority: depth })),
    ...layerHighlights(layer.layers, depth + 1),
  ]);
}

function BundleInventory({
  result,
  files,
  sourceInputs,
}: {
  result: PlaygroundResponse | null;
  files: { path: string; sourcePath: string }[];
  sourceInputs: { path: string }[];
}) {
  const queryPaths = result?.bundle.query_paths ?? files.filter((file) => file.path.startsWith("queries/")).map((file) => file.path);
  const corpusPaths =
    result?.bundle.corpus_paths ??
    files
      .filter(
        (file) =>
          file.path.startsWith("test/corpus/") ||
          file.path.startsWith("test/highlight/") ||
          file.path.startsWith("test/highlights/"),
      )
      .map((file) => file.path);
  const sourcePaths = sourceInputs.map((file) => file.path);
  const scannerPaths = result?.bundle.scanner_paths ?? [];
  const ignoredPaths =
    result?.bundle.generated_files_ignored ?? files.filter((file) => isGeneratedPath(file.path)).map((file) => file.path);
  const grammarPath =
    result?.bundle.grammar_path ?? (files.some((file) => file.path === "src/grammar.json") ? "src/grammar.json" : null);
  const grammarJsPath =
    result?.bundle.grammar_js_path ?? (files.some((file) => file.path === "grammar.js") ? "grammar.js" : null);
  const grammarSources = [
    grammarPath ? `grammar: ${grammarPath}` : null,
    grammarJsPath ? (grammarPath ? `source DSL: ${grammarJsPath}` : `grammar: ${grammarJsPath}`) : null,
  ].filter((path): path is string => path !== null);
  const activeScanner = result?.bundle.active_scanner ? [result.bundle.active_scanner] : [];

  return (
    <details className="inventory">
      <summary>
        Bundle inventory
        <span className="inventory-counts">
          {queryPaths.length} queries · {corpusPaths.length} tests · {sourcePaths.length} source
        </span>
      </summary>
      <div className="inventory-body">
        <InventoryGroup title="Grammar" paths={grammarSources} />
        <InventoryGroup title="Queries" paths={queryPaths} />
        <InventoryGroup title="Corpus & highlights" paths={corpusPaths} />
        <InventoryGroup title="Source inputs" paths={sourcePaths} />
        <InventoryGroup title="Active scanner" paths={activeScanner} />
        <InventoryGroup title="Scanners" paths={scannerPaths} />
        <InventoryGroup title="Ignored (generated)" paths={ignoredPaths} />
      </div>
    </details>
  );
}

function InventoryGroup({ title, paths }: { title: string; paths: string[] }) {
  if (!paths.length) {
    return null;
  }
  return (
    <div className="inventory-group">
      <h3>{title}</h3>
      <ul>
        {paths.map((path) => (
          <li key={path}>
            <code>{path}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}

function rawBrowserPath(file: File) {
  const relative = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
  return normalizePath(relative && relative.length > 0 ? relative : file.name);
}

function responseWithDiagnostic(stage: string, message: string, files: BundleFile[]): PlaygroundResponse {
  return {
    ok: false,
    language: null,
    diagnostics: [{ stage, message, primary_span: null }],
    bundle: {
      grammar_path: files.some((file) => file.path === "src/grammar.json") ? "src/grammar.json" : null,
      grammar_js_path: files.some((file) => file.path === "grammar.js") ? "grammar.js" : null,
      query_paths: files.filter((file) => file.path.startsWith("queries/")).map((file) => file.path),
      corpus_paths: files
        .filter(
          (file) =>
            file.path.startsWith("test/corpus/") ||
            file.path.startsWith("test/highlight/") ||
            file.path.startsWith("test/highlights/"),
        )
        .map((file) => file.path),
      sample_paths: files.filter((file) => file.path.startsWith("samples/")).map((file) => file.path),
      generated_files_ignored: files.filter((file) => isGeneratedPath(file.path)).map((file) => file.path),
      scanner_paths: files
        .filter((file) => file.path === "src/scanner.c" || file.path === "src/scanner.cc")
        .map((file) => file.path),
      active_scanner: null,
    },
    plan: null,
    parse: null,
    highlights: [],
    injections: [],
    layers: [],
    corpus: [],
    highlight_tests: [],
    tests: {
      requested: false,
      corpus_passed: 0,
      corpus_failed: 0,
      highlight_assertions_passed: 0,
      highlight_assertions_failed: 0,
      highlight_fixture_errors: 0,
    },
    timings: { prepare: [], parse: null },
  };
}

class PlaygroundRunError extends Error {
  readonly stage: string;

  constructor(stage: string, message: string) {
    super(message);
    this.stage = stage;
  }
}

function sessionCacheKey(grammarRootId: string, files: BundleFile[]) {
  return JSON.stringify({
    grammarRootId,
    files: sortedFiles(files).map(({ path, text }) => [path, text]),
  });
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isGeneratedPath(path: string) {
  return (
    [
      "src/parser.c",
      "src/parser.cc",
      "src/parser.h",
      "src/node-types.json",
      "bindings/node/binding.cc",
    ].includes(path) ||
    path.endsWith("/src/parser.c") ||
    path.endsWith("/src/parser.cc") ||
    path.endsWith("/src/parser.h") ||
    path.endsWith("/src/node-types.json") ||
    path.endsWith("/bindings/node/binding.cc")
  );
}

function sortedRuntimeBundleFiles(files: ProjectedDslBundleFile[]) {
  const runtimeFiles = sortedFiles(files).filter((file) => isRuntimeBundlePath(file.path));
  const samples = sortedSampleFiles(runtimeFiles.filter((file): file is SampleFile => file.path.startsWith("samples/")));
  return [...runtimeFiles.filter((file) => !file.path.startsWith("samples/")), ...samples];
}

function isRuntimeBundlePath(path: string) {
  return (
    path === "tree-sitter.json" ||
    path === "grammar.js" ||
    path === "src/grammar.json" ||
    path === "src/scanner.c" ||
    path === "src/scanner.cc" ||
    (path.endsWith(".js") && !isGeneratedPath(path)) ||
    path.startsWith("queries/") ||
    path.startsWith("test/corpus/") ||
    path.startsWith("test/highlight/") ||
    path.startsWith("test/highlights/") ||
    path.startsWith("samples/")
  );
}

function placedDiagnostic(result: PlaygroundResponse | null) {
  return result?.diagnostics.find((candidate) => candidate.primary_span) ?? null;
}
