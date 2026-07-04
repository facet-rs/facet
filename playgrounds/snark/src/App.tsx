import { type CSSProperties, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { runParse } from "./parseClient";
import { runBenchmark, BenchBody, benchMeta, type BenchReport } from "./benchmark";
import { SourceEditor, type EditorJump, type IdeInfo, type IdeState, type SourceEdit } from "./editor";
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
  lexer_call_count: number;
  lexer_direct_set_cache_hits: number;
  lexer_direct_set_cache_misses: number;
  lexer_stencil_executions: ParseLexerStencilExecutionOutput[];
  dominant_lexer_stencil_execution: ParseLexerStencilExecutionOutput | null;
  execution_lane: string;
  snark_intrinsic_count: number;
  snark_stencil_executions: ParseSnarkStencilExecutionOutput[];
  dominant_snark_stencil_execution: ParseSnarkStencilExecutionOutput | null;
  trace_event_count: number;
  tree_event_count: number;
  reuse_node_count: number;
  accepted_tree_event_count: number;
  accepted_error_count: number;
  accepted_missing_count: number;
};

type ParseLexerStencilExecutionOutput = {
  kind: string;
  count: number;
};

type ParseSnarkStencilExecutionOutput = {
  family: string;
  execution: string;
  count: number;
};

type PlanOutput = {
  fully_visible: boolean;
  parser_fully_visible: boolean;
  lexer_fully_visible: boolean;
  neutral_weavy_only: boolean;
  stencils_needed: boolean;
  lexer_stencils_needed: boolean;
  copy_patch_jit_available: boolean;
  neutral_weavy_op_count: number;
  snark_intrinsic_count: number;
  snark_stencils: PlanStencilOutput[];
  lexer_stencils: PlanLexerStencilOutput[];
  snark_stencil_families: PlanStencilFamilyOutput[];
  snark_stencil_executions: PlanStencilExecutionOutput[];
  snark_stencil_states: PlanStencilStateOutput[];
  backend_executions: PlanBackendExecutionOutput[];
  dominant_backend_execution: PlanBackendExecutionOutput | null;
  lowering_barriers: PlanBarrierOutput[];
};

type PlanBackendExecutionOutput = {
  execution: string;
  parser_count: number;
  lexer_count: number;
  total_count: number;
};

type PlanLexerStencilOutput = {
  kind: string;
  execution: string;
  state: string[];
  count: number;
};

type PlanStencilFamilyOutput = {
  family: string;
  execution: string;
  state: string[];
  effect: PlanStencilEffectOutput;
  count: number;
};

type PlanStencilExecutionOutput = {
  execution: string;
  families: string[];
  state: string[];
  effect: PlanStencilEffectOutput;
  count: number;
};

type PlanStencilStateOutput = {
  state: string;
  count: number;
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
  /** vix IDE bindings (attached client-side from the worker's vix lane). */
  vix_ide?: IdeState;
  /** vix machine run (attached client-side from the worker's vix lane). */
  vix_machine?: VixMachineRun | null;
};

type VixMachineRun = {
  ok: boolean;
  error: string | null;
  source_kind: string;
  fn_name: string;
  result: VixMachineResult | null;
  cold_trace: VixDriveEvent[];
  warm_trace: VixDriveEvent[];
  fn_hashes: HashLabel[];
  run_hashes: HashLabel[];
};

type VixMachineResult = {
  schema: string;
  i64_value: number | null;
  f64_value: number | null;
  tree_entries: TreeEntry[];
};

type TreeEntry = {
  path: string;
  contents: string;
};

type HashLabel = {
  hash: string;
  label: string;
};

type VixSpan = {
  start: number;
  end: number;
};

type RunOutput = {
  path: string;
  hash: string;
};

type VixExecServing =
  | { type: "Tier1Hit" }
  | { type: "Tier2Cutoff"; verified: number }
  | { type: "Ran" }
  | { type: "Joined" };

type VixDriveEvent =
  | { type: "Demanded"; fn_hash: string }
  | { type: "MemoHit"; fn_hash: string }
  | { type: "Spawned"; fn_hash: string }
  | { type: "ParkedOn"; fn_hash: string }
  | { type: "Completed"; fn_hash: string }
  | { type: "SpawnedInvocation"; fn_hash: string; key_hash: string }
  | { type: "StoreAlloc"; schema_ref: string; deduped: boolean }
  | {
      type: "RunRequested";
      command: string;
      output: string;
      run_id: number;
      command_name: string;
      argv: string[];
      describe: string[];
      span: VixSpan | null;
      timestamp_us: number;
    }
  | {
      type: "RunStarted";
      command: string;
      output: string;
      run_id: number;
      command_name: string;
      timestamp_us: number;
    }
  | {
      type: "RunCompleted";
      command: string;
      output: string;
      run_id: number;
      command_name: string;
      serving: VixExecServing;
      outputs: RunOutput[];
      timestamp_us: number;
    }
  | {
      type: "Observation";
      key: string;
      replayed: boolean;
      key_text: string;
      timestamp_us: number;
    };

const defaultFiles: BundleFile[] = vendoredFiles;
// One frame (~60fps). Leading-edge throttle interval for live re-parsing.
const PARSE_THROTTLE_MS = 16;

// Hash routes: `#/vix` selects a grammar root, `#/vix/samples/lua.vix` a sample too.
// Kept in sync both ways (replaceState on UI changes; hashchange applies inbound edits).
function parseHashRoute(): { root: string; sample: string } | null {
  const raw = window.location.hash.replace(/^#\/?/, "");
  if (!raw) return null;
  const [root, ...rest] = raw.split("/");
  if (!root) return null;
  return { root: decodeURIComponent(root), sample: rest.map(decodeURIComponent).join("/") };
}

function writeHashRoute(root: string, sample: string) {
  const suffix = sample ? `/${sample.split("/").map(encodeURIComponent).join("/")}` : "";
  window.history.replaceState(null, "", `#/${encodeURIComponent(root)}${suffix}`);
}

const initialRoute = parseHashRoute();
const initialRouteValid =
  initialRoute != null && discoverGrammarRoots(defaultFiles).some((root) => root.id === initialRoute.root);
const defaultGrammarRoot = initialRouteValid ? initialRoute.root : defaultVendoredRootId;
const routedSample = initialRouteValid
  ? sourceExamplesForGrammarRootId(defaultFiles, defaultGrammarRoot).find(
      (file) => file.path === initialRoute.sample,
    )
  : undefined;
const defaultSample =
  routedSample ??
  sourceExamplesForGrammarRootId(defaultFiles, defaultGrammarRoot).find(
    (file) => defaultGrammarRoot === "vix" && file.path === "samples/lua.vix",
  ) ??
  sourceExamplesForGrammarRootId(defaultFiles, defaultGrammarRoot).find(
    (file) => defaultGrammarRoot === "vix" && file.path === "samples/merge-demand.vix",
  ) ??
  preferredSampleForGrammarRootId(defaultFiles, defaultGrammarRoot);

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
  const [machineFn, setMachineFn] = useState(defaultMachineFnForInput(defaultSample?.text ?? ""));
  const [editorJump, setEditorJump] = useState<EditorJump | null>(null);
  const [sourceCursorByte, setSourceCursorByte] = useState(0);
  const [traceOpen, setTraceOpen] = useState(false);
  const [tracePhase, setTracePhase] = useState<TracePhase>("cold");
  const [traceView, setTraceView] = useState<TraceView>("dag");
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [hoveredTraceId, setHoveredTraceId] = useState<string | null>(null);
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

  const applyGrammarRoot = (nextGrammarRoot: string, samplePath?: string) => {
    const sample =
      (samplePath
        ? sourceExamplesForGrammarRootId(files, nextGrammarRoot).find((file) => file.path === samplePath)
        : undefined) ?? preferredSampleForGrammarRootId(files, nextGrammarRoot);
    autoTestedKeyRef.current = null;
    bundledTestSnapshotRef.current = null;
    preparedKeyRef.current = null;
    baselineInputRef.current = null;
    pendingSourceEditRef.current = null;
    setSelectedGrammarRoot(nextGrammarRoot);
    setSelectedSamplePath(sample?.path ?? "");
    setInput(sample?.text ?? "");
    setMachineFn(defaultMachineFnForInput(sample?.text ?? ""));
    setResult(null);
    writeHashRoute(nextGrammarRoot, sample?.path ?? "");
  };
  const applyGrammarRootRef = useRef(applyGrammarRoot);
  applyGrammarRootRef.current = applyGrammarRoot;

  // Inbound routing: editing the URL hash (or following a #/vix link) applies the route.
  useEffect(() => {
    const onHashChange = () => {
      const route = parseHashRoute();
      if (!route) return;
      applyGrammarRootRef.current(route.root, route.sample || undefined);
    };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);
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
  const machineOptions = useMemo(
    () => (activeGrammarRootId === "vix" ? vixMachineOptions(input) : []),
    [activeGrammarRootId, input],
  );
  const activeMachineFn = machineOptions.some((option) => option.name === machineFn)
    ? machineFn
    : (machineOptions[0]?.name ?? "");
  const traceModel = useMemo(
    () => (result?.vix_machine?.ok ? buildTraceModel(result.vix_machine, tracePhase) : null),
    [result?.vix_machine, tracePhase],
  );

  useEffect(() => {
    if (activeMachineFn && activeMachineFn !== machineFn) {
      setMachineFn(activeMachineFn);
    }
  }, [activeMachineFn, machineFn]);

  useEffect(() => {
    setTracePhase("cold");
    setSelectedTraceId(null);
    setHoveredTraceId(null);
  }, [result?.vix_machine]);

  useEffect(() => {
    if (!traceModel || selectedTraceId === null || traceModel.entities.has(selectedTraceId)) {
      return;
    }
    setSelectedTraceId(traceModel.entities.values().next().value?.id ?? null);
  }, [selectedTraceId, traceModel]);

  useEffect(() => {
    if (!traceOpen) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setTraceOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [traceOpen]);

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
  }, [activeGrammarRootId, activeMachineFn, files, hasBundledTests, input, projectedFiles]);

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
    setMachineFn(defaultMachineFnForInput(nextSample?.text ?? ""));
    setResult(null);
  }

  function updateSourceInput(nextInput: string, samplePath = "", edit: SourceEdit | null = null) {
    pendingSourceEditRef.current = edit ? { oldInput: input, edit } : null;
    setInput(nextInput);
    setSelectedSamplePath(samplePath);
    if (!edit) {
      setMachineFn(defaultMachineFnForInput(nextInput));
    }
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

      let result: { response: string; prepared: boolean; vix: string | null; vixMachine: string | null };
      try {
        result = await runParse({
          key,
          files: runnableFiles,
          input,
          runCorpus: runBundledTests,
          edit: useReparse ? pendingEdit.edit : null,
          useReparse,
          vixIde: activeGrammarRootId === "vix",
          vixMachineFn: activeGrammarRootId === "vix" ? activeMachineFn : null,
        });
      } catch (error) {
        // A worker/prepare failure: force a fresh prepare on the next run.
        preparedKeyRef.current = null;
        baselineInputRef.current = null;
        throw new PlaygroundRunError("snark", errorMessage(error));
      }

      preparedKeyRef.current = result.prepared ? key : null;
      const parsed = JSON.parse(result.response) as PlaygroundResponse;
      // IDE bindings only make sense against the exact input they were computed for.
      parsed.vix_ide = result.vix ? { ide: JSON.parse(result.vix) as IdeInfo, input } : null;
      parsed.vix_machine = result.vixMachine ? (JSON.parse(result.vixMachine) as VixMachineRun) : null;
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
              🍉
            </span>
            <div className="brand-text">
              <h1>snark</h1>
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
                onChange={(event) => applyGrammarRoot(event.currentTarget.value)}
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
                    writeHashRoute(selectedGrammarRoot, sample.path);
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
            {machineOptions.length > 0 ? (
              <select
                aria-label="Run on machine"
                className="select"
                value={activeMachineFn}
                onChange={(event) => setMachineFn(event.currentTarget.value)}
              >
                {machineOptions.map((option) => (
                  <option key={option.name} value={option.name}>
                    machine · {option.name}
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

        {traceOpen && traceModel ? (
          <MachineTraceExperience
            model={traceModel}
            traceView={traceView}
            setTraceView={setTraceView}
            selectedId={selectedTraceId}
            hoveredId={hoveredTraceId}
            onSelect={setSelectedTraceId}
            onHover={setHoveredTraceId}
            onPhase={setTracePhase}
            onClose={() => setTraceOpen(false)}
            onSourceSpan={(span) => {
              setTraceOpen(false);
              setEditorJump((current) => ({
                start_byte: span.start,
                end_byte: span.end,
                nonce: (current?.nonce ?? 0) + 1,
              }));
            }}
          />
        ) : (
          <SourceEditor
            input={input}
            captures={editorCaptures}
            diagnostic={editorDiagnostic}
            ide={result?.vix_ide ?? null}
            jump={editorJump}
            onCursorByte={(byte) => {
              setSourceCursorByte(byte);
              const hit = traceModel ? entityAtSourceByte(traceModel, byte) : null;
              if (hit) {
                setSelectedTraceId(hit.id);
              }
            }}
            onChange={(value, edit) => updateSourceInput(value, "", edit)}
          />
        )}

        <ResultsDock
          result={result}
          machine={result?.vix_machine ?? null}
          traceModel={traceModel}
          sourceCursorByte={sourceCursorByte}
          onOpenTrace={() => setTraceOpen(true)}
          onUseInput={(value, sourcePath = "") => updateSourceInput(value, sourcePath)}
          bench={{
            report: benchReport,
            running: busyTask === "bench",
            progress: benchProgress,
            onRun: () => void handleRunBenchmark(),
          }}
        />
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


type DockSection = {
  id: string;
  title: string;
  meta?: ReactNode;
  body: ReactNode;
};

// One slim chip bar at the very bottom; at most ONE drawer open above it. Quiet by default:
// nothing is expanded until asked, and the editor owns the viewport.
function ResultsDock({
  result,
  machine,
  traceModel,
  sourceCursorByte,
  onOpenTrace,
  onUseInput,
  bench,
}: {
  result: PlaygroundResponse | null;
  machine: VixMachineRun | null;
  traceModel: TraceModel | null;
  sourceCursorByte: number;
  onOpenTrace: () => void;
  onUseInput: (value: string, sourcePath?: string) => void;
  bench: { report: BenchReport | null; running: boolean; progress: string; onRun: () => void };
}) {
  const [active, setActive] = useState<string | null>(null);
  const failure = result && !result.ok ? result : null;
  const sexp = result?.parse?.sexp ?? "";
  const captures = composedHighlights(result);
  const layers = result?.layers ?? [];
  const plan = result?.plan ?? null;
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

  const sections: DockSection[] = [];

  if (timingRows.length) {
    sections.push({
      id: "timings",
      title: "Timings",
      meta: result?.timings?.parse ? `${result.timings.parse.ms.toFixed(2)} ms` : "prepare only",
      body: (
        <>
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
            Prepare phases run once per grammar; “run parser” is live per input — watch it stay flat
            as input grows.
          </p>
        </>
      ),
    });
  }

  if (plan) {
    const dominant = plan.dominant_backend_execution;
    sections.push({
      id: "plan",
      title: "Plan",
      meta: dominant ? `${dominant.execution} · ${dominant.total_count} ops` : "no backend lane",
      body: <PlanBody plan={plan} parse={result?.parse ?? null} />,
    });
  }

  sections.push({
    id: "sexp",
    title: "S-expression",
    meta: result?.parse ? (
      <>
        {result.parse.accepted_count} accepted · {result.parse.failure_count} failed
        {result.parse.reuse_node_count ? ` · ${result.parse.reuse_node_count} reused` : ""}
        {result.parse.accepted_error_count || result.parse.accepted_missing_count
          ? ` · ${result.parse.accepted_error_count} ERROR · ${result.parse.accepted_missing_count} MISSING`
          : ""}
      </>
    ) : undefined,
    body: sexp ? <pre className="sexp">{sexp}</pre> : <p className="empty">No parse tree.</p>,
  });

  sections.push({
    id: "captures",
    title: "Captures",
    meta: captures.length,
    body: captures.length ? (
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
    ),
  });

  if (layers.length) {
    sections.push({
      id: "layers",
      title: "Layers",
      meta: countLayers(layers),
      body: <LayerList layers={layers} />,
    });
  }

  if (tests) {
    sections.push({
      id: "tests",
      title: "Tests",
      meta: (
        <>
          {tests.tests.corpus_passed + tests.tests.highlight_assertions_passed} pass ·{" "}
          {tests.tests.corpus_failed +
            tests.tests.highlight_assertions_failed +
            tests.tests.highlight_fixture_errors}{" "}
          fail
        </>
      ),
      body: (
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
      ),
    });
  }

  sections.push({
    id: "bench",
    title: "Benchmark",
    meta: bench.running ? `running… ${bench.progress}` : benchMeta(bench.report),
    body: <BenchBody {...bench} />,
  });

  const activeSection = sections.find((section) => section.id === active) ?? null;
  const sourceHit = traceModel ? entityAtSourceByte(traceModel, sourceCursorByte) : null;

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

      {activeSection ? (
        <div className="dock-drawer">
          <div className="panel-body">{activeSection.body}</div>
        </div>
      ) : null}

      {machine ? (
        <div className="machine-summary-strip">
          <span>{machine.ok ? `${machine.fn_name} · ${machine.cold_trace.length}/${machine.warm_trace.length} events` : `${machine.fn_name} blocked`}</span>
          <code>
            {machine.ok && traceModel
              ? `${traceModel.runs.length} runs · ${traceModel.observations.length} observations`
              : machine.error ?? "machine run failed"}
          </code>
          {sourceHit ? <small>source touches {sourceHit.label}</small> : null}
          <button type="button" className="ghost" onClick={onOpenTrace} disabled={!machine.ok}>
            Open trace
          </button>
        </div>
      ) : null}

      <nav className="dock-bar" aria-label="Result panels">
        {sections.map((section) => (
          <button
            type="button"
            key={section.id}
            className={`dock-chip ${active === section.id ? "on" : ""}`}
            aria-expanded={active === section.id}
            onClick={() => setActive((current) => (current === section.id ? null : section.id))}
          >
            <span className="dock-chip-title">{section.title}</span>
            {section.meta != null ? <span className="dock-chip-meta">{section.meta}</span> : null}
          </button>
        ))}
      </nav>
    </div>
  );
}

type MachineRunSummary = {
  runId: number;
  command: string;
  commandName: string;
  output: string;
  outputLabel: string;
  argv: string[];
  describe: string[];
  span: VixSpan | null;
  requestedAt: number | null;
  startedAt: number | null;
  completedAt: number | null;
  serving: VixExecServing | null;
  outputs: RunOutput[];
};

type TracePhase = "cold" | "warm";
type TraceView = "dag" | "timeline" | "runs";
type TraceEntityKind = "invocation" | "run" | "observation" | "pending";
type TraceVisual = "spawn" | "memo" | "run" | "ran" | "tier1" | "tier2" | "joined" | "observation" | "replayed" | "pending";

type TraceEntity = {
  id: string;
  kind: TraceEntityKind;
  visual: TraceVisual;
  label: string;
  detail: string;
  startIndex: number;
  endIndex: number;
  startUs: number | null;
  endUs: number | null;
  span: VixSpan | null;
  fnHash?: string;
  keyHash?: string;
  run?: MachineRunSummary;
  observation?: Extract<VixDriveEvent, { type: "Observation" }>;
  chips: string[];
};

type TraceEdge = {
  id: string;
  from: string;
  to: string;
  label: string;
  kind: "demand" | "spawn" | "run" | "await" | "observe" | "ref";
};

type TraceModel = {
  phase: TracePhase;
  fnName: string;
  trace: VixDriveEvent[];
  resultSummary: string[];
  entities: Map<string, TraceEntity>;
  invocations: TraceEntity[];
  runs: TraceEntity[];
  observations: TraceEntity[];
  pending: TraceEntity[];
  edges: TraceEdge[];
  timelineEndUs: number;
};

type TraceProjectionProps = {
  model: TraceModel;
  selectedId: string | null;
  hoveredId: string | null;
  onSelect: (id: string) => void;
  onHover: (id: string | null) => void;
};

function MachineTraceExperience({
  model,
  traceView,
  setTraceView,
  selectedId,
  hoveredId,
  onSelect,
  onHover,
  onPhase,
  onClose,
  onSourceSpan,
}: {
  model: TraceModel;
  traceView: TraceView;
  setTraceView: (view: TraceView) => void;
  selectedId: string | null;
  hoveredId: string | null;
  onSelect: (id: string | null) => void;
  onHover: (id: string | null) => void;
  onPhase: (phase: TracePhase) => void;
  onClose: () => void;
  onSourceSpan: (span: VixSpan) => void;
}) {
  const activeEntity = selectedId ? model.entities.get(selectedId) ?? null : null;
  const visibleEntity = (hoveredId ? model.entities.get(hoveredId) : null) ?? activeEntity ?? model.entities.values().next().value ?? null;

  const projectionProps: TraceProjectionProps = {
    model,
    selectedId,
    hoveredId,
    onSelect,
    onHover,
  };

  return (
    <div className="trace-takeover">
      <header className="trace-head">
        <div className="trace-title">
          <strong>{model.fnName}</strong>
          <span>{model.phase} machine trace</span>
        </div>
        <div className="trace-tabs" role="tablist" aria-label="Trace view">
          {(["dag", "timeline", "runs"] as TraceView[]).map((view) => (
            <button
              type="button"
              role="tab"
              aria-selected={traceView === view}
              className={traceView === view ? "on" : ""}
              key={view}
              onClick={() => setTraceView(view)}
            >
              {view}
            </button>
          ))}
        </div>
        <div className="trace-actions">
          <button type="button" className="ghost" onClick={() => onPhase(model.phase === "cold" ? "warm" : "cold")}>
            {model.phase === "cold" ? "Demand again" : "Cold trace"}
          </button>
          <button type="button" className="ghost" onClick={onClose}>
            Editor
          </button>
        </div>
      </header>

      <div className="trace-result">
        {model.resultSummary.map((entry) => (
          <code key={entry}>{entry}</code>
        ))}
      </div>

      <div className={`trace-work trace-work-${traceView}`}>
        {traceView === "dag" ? <TraceDag {...projectionProps} /> : null}
        {traceView === "timeline" ? <TraceTimeline {...projectionProps} /> : null}
        {traceView === "runs" ? <TraceRuns {...projectionProps} onSourceSpan={onSourceSpan} /> : null}
        <TraceDetail entity={visibleEntity} onSourceSpan={onSourceSpan} />
      </div>
    </div>
  );
}

function buildTraceModel(run: VixMachineRun, phase: TracePhase): TraceModel {
  const trace = phase === "warm" ? run.warm_trace : run.cold_trace;
  const fnLabels = labelMap(run.fn_hashes);
  const runLabels = labelMap(run.run_hashes);
  const runSummaries = new Map(runLifecycle(trace, runLabels).map((event) => [event.runId, event]));
  const entities = new Map<string, TraceEntity>();
  const invocations: TraceEntity[] = [];
  const runs: TraceEntity[] = [];
  const observations: TraceEntity[] = [];
  const pending: TraceEntity[] = [];
  const edges: TraceEdge[] = [];
  let root: TraceEntity | null = null;
  let latestInvocation: TraceEntity | null = null;
  let latestObject: TraceEntity | null = null;
  const latestRunById = new Map<number, string>();
  const materializedOutputs = new Set<string>();

  const addEntity = (entity: TraceEntity) => {
    entities.set(entity.id, entity);
    if (entity.kind === "invocation") invocations.push(entity);
    if (entity.kind === "run") runs.push(entity);
    if (entity.kind === "observation") observations.push(entity);
    if (entity.kind === "pending") pending.push(entity);
    return entity;
  };
  const addEdge = (from: TraceEntity | null, to: TraceEntity, label: string, kind: TraceEdge["kind"]) => {
    if (from && from.id !== to.id && !edges.some((edge) => edge.from === from.id && edge.to === to.id && edge.label === label)) {
      edges.push({ id: `${from.id}-${to.id}-${label}`, from: from.id, to: to.id, label, kind });
    }
  };
  const latestByFn = new Map<string, TraceEntity[]>();
  const pushInvocation = (event: Extract<VixDriveEvent, { fn_hash: string }>, index: number) => {
    const label = fnLabels.get(event.fn_hash) ?? shortHash(event.fn_hash);
    const entity = addEntity({
      id: `inv-${index}`,
      kind: "invocation",
      visual: "spawn",
      label,
      detail: "demanded",
      startIndex: index,
      endIndex: index,
      startUs: null,
      endUs: null,
      span: null,
      fnHash: event.fn_hash,
      chips: ["demand"],
    });
    latestByFn.set(event.fn_hash, [...(latestByFn.get(event.fn_hash) ?? []), entity]);
    root ??= entity;
    latestInvocation = entity;
    if (label === "object") latestObject = entity;
    return entity;
  };
  const openInvocation = (event: Extract<VixDriveEvent, { fn_hash: string }>, index: number) => {
    const candidates = latestByFn.get(event.fn_hash) ?? [];
    for (let i = candidates.length - 1; i >= 0; i -= 1) {
      const entity = candidates[i];
      if (!entity.chips.includes("complete") && !entity.chips.includes("memo")) {
        return entity;
      }
    }
    return pushInvocation(event, index);
  };

  trace.forEach((event, index) => {
    if (event.type === "Demanded") {
      const entity = pushInvocation(event, index);
      addEdge(root && root.id !== entity.id ? root : null, entity, "demand", "demand");
    } else if (event.type === "Spawned" || event.type === "MemoHit" || event.type === "ParkedOn" || event.type === "Completed") {
      const entity = openInvocation(event, index);
      entity.endIndex = index;
      if (event.type === "Spawned") {
        entity.detail = "spawned";
        entity.visual = "spawn";
        if (!entity.chips.includes("spawn")) entity.chips.push("spawn");
        addEdge(root && root.id !== entity.id ? root : null, entity, phase === "warm" ? "memo" : "spawn", "spawn");
      } else if (event.type === "MemoHit") {
        entity.detail = "memo hit";
        entity.visual = "memo";
        entity.chips = [...new Set([...entity.chips, "memo"])];
      } else if (event.type === "ParkedOn") {
        entity.detail = "awaiting child";
        entity.chips = [...new Set([...entity.chips, "await"])];
      } else {
        entity.detail = entity.visual === "memo" ? "memo hit" : "completed";
        entity.chips = [...new Set([...entity.chips, "complete"])];
      }
      latestInvocation = entity;
    } else if (event.type === "SpawnedInvocation" && latestInvocation) {
      latestInvocation.keyHash = event.key_hash;
      latestInvocation.detail = `key ${shortHash(event.key_hash)}`;
    } else if (
      event.type === "RunRequested" ||
      event.type === "RunStarted" ||
      event.type === "RunCompleted"
    ) {
      const summary = runSummaries.get(event.run_id);
      const output = summary?.outputLabel ?? runLabels.get(event.output) ?? event.output;
      materializedOutputs.add(output);
      for (const output of summary?.outputs ?? []) {
        materializedOutputs.add(output.path);
      }
      const existing = latestRunById.get(event.run_id);
      let entity = existing ? entities.get(existing) ?? null : null;
      if (!entity) {
        entity = addEntity({
          id: `run-${event.run_id}`,
          kind: "run",
          visual: summary?.serving ? servingClass(summary.serving) : "run",
          label: output,
          detail: summary ? `${summary.commandName} · ${runStatus(summary)}` : event.type.replace(/^Run/, "").toLowerCase(),
          startIndex: index,
          endIndex: index,
          startUs: summary?.startedAt ?? summary?.requestedAt ?? event.timestamp_us,
          endUs: summary?.completedAt ?? summary?.startedAt ?? summary?.requestedAt ?? event.timestamp_us,
          span: summary?.span ?? null,
          run: summary,
          chips: summary?.serving ? [servingLabel(summary.serving)] : [runStatus(summary ?? emptyRunSummary(event, runLabels))],
        });
        latestRunById.set(event.run_id, entity.id);
      }
      if (summary) {
        entity.run = summary;
        entity.label = summary.outputLabel;
        entity.visual = summary.serving ? servingClass(summary.serving) : "run";
        entity.detail = summary.serving
          ? `${servingLabel(summary.serving)} · ${summary.commandName}`
          : event.type.replace(/^Run/, "").toLowerCase();
        entity.startUs = summary.startedAt ?? summary.requestedAt ?? event.timestamp_us;
        entity.endUs = summary.completedAt ?? summary.startedAt ?? summary.requestedAt ?? event.timestamp_us;
        entity.span = summary.span;
        entity.chips = summary.serving ? [servingLabel(summary.serving)] : [runStatus(summary)];
      }
      entity.endIndex = index;
      addEdge(latestObject ?? latestInvocation ?? root, entity, "run", "run");
    } else if (event.type === "Observation") {
      const entity = addEntity({
        id: `observation-${index}`,
        kind: "observation",
        visual: event.replayed ? "replayed" : "observation",
        label: event.replayed ? "observe replay" : "observe",
        detail: event.key_text || shortHash(event.key),
        startIndex: index,
        endIndex: index,
        startUs: event.timestamp_us,
        endUs: event.timestamp_us,
        span: null,
        observation: event,
        chips: [event.replayed ? "replayed" : "cold"],
      });
      addEdge(latestInvocation ?? root, entity, event.replayed ? "replay" : "observe", "observe");
    }
  });

  for (const pending of pendingOutputs(run)) {
    if (materializedOutputs.has(pending)) {
      continue;
    }
    const entity = addEntity({
      id: `pending-${pending}`,
      kind: "pending",
      visual: "pending",
      label: pending,
      detail: "present, never lit",
      startIndex: trace.length,
      endIndex: trace.length,
      startUs: null,
      endUs: null,
      span: null,
      chips: ["PENDING"],
    });
    addEdge(root, entity, "ref", "ref");
  }

  return {
    phase,
    fnName: run.fn_name,
    trace,
    resultSummary: machineResultSummary(run),
    entities,
    invocations,
    runs,
    observations,
    pending,
    edges,
    timelineEndUs: Math.max(1, ...trace.map((event, index) => eventTimestamp(event) ?? index)),
  };
}

function TraceDag({ model, selectedId, hoveredId, onSelect, onHover }: TraceProjectionProps) {
  const layout = layoutTraceDag(model);
  return (
    <div className="trace-dag" style={{ minWidth: layout.width, minHeight: layout.height }}>
      <svg className="trace-dag-lines" width={layout.width} height={layout.height} aria-hidden="true">
        {model.edges.map((edge) => {
          const from = layout.nodes.get(edge.from);
          const to = layout.nodes.get(edge.to);
          if (!from || !to) return null;
          const x1 = from.x + from.width;
          const y1 = from.y + from.height / 2;
          const x2 = to.x;
          const y2 = to.y + to.height / 2;
          const mid = x1 + Math.max(36, (x2 - x1) / 2);
          return (
            <g className={`trace-edge trace-edge-${edge.kind}`} key={edge.id}>
              <path d={`M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}`} />
              <text x={(x1 + x2) / 2} y={(y1 + y2) / 2 - 5}>{edge.label}</text>
            </g>
          );
        })}
      </svg>
      {Array.from(model.entities.values()).map((entity) => {
        const at = layout.nodes.get(entity.id);
        if (!at) return null;
        return (
          <TraceEntityButton
            entity={entity}
            model={model}
            selectedId={selectedId}
            hoveredId={hoveredId}
            onSelect={onSelect}
            onHover={onHover}
            className="trace-dag-node"
            style={{ transform: `translate(${at.x}px, ${at.y}px)`, width: at.width, height: at.height }}
            key={entity.id}
          />
        );
      })}
    </div>
  );
}

function TraceTimeline({ model, selectedId, hoveredId, onSelect, onHover }: TraceProjectionProps) {
  const lanes = timelineLanes(model);
  return (
    <div className="trace-timeline">
      {lanes.map((lane) => (
        <div className="trace-lane" key={lane.name}>
          <span>{lane.name}</span>
          <div className="trace-lane-track">
            {lane.entities.map((entity) => {
              const [left, width] = timelineRange(model, entity);
              return (
                <button
                  type="button"
                  className={traceEntityClass(entity, model, selectedId, hoveredId)}
                  style={{ left: `${left}%`, width: `${width}%` }}
                  key={entity.id}
                  onClick={() => onSelect(entity.id)}
                  onMouseEnter={() => onHover(entity.id)}
                  onMouseLeave={() => onHover(null)}
                  title={`${entity.label}: ${entity.detail}`}
                >
                  <span>{entity.label}</span>
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

function TraceRuns({ model, selectedId, hoveredId, onSelect, onHover, onSourceSpan }: TraceProjectionProps & { onSourceSpan: (span: VixSpan) => void }) {
  return (
    <div className="trace-runs">
      {model.runs.map((entity) => {
        const run = entity.run;
        return (
          <button
            type="button"
            className={traceEntityClass(entity, model, selectedId, hoveredId)}
            key={entity.id}
            onClick={() => onSelect(entity.id)}
            onMouseEnter={() => onHover(entity.id)}
            onMouseLeave={() => onHover(null)}
          >
            <span>run #{run?.runId ?? "?"}</span>
            <code>{entity.label}</code>
            <strong>{run?.serving ? servingLabel(run.serving) : run ? runStatus(run) : entity.detail}</strong>
            <small>{run ? runTimestamps(run) : ""}</small>
            {entity.span ? (
              <span
                className="trace-source-chip"
                onClick={(event) => {
                  event.stopPropagation();
                  onSourceSpan(entity.span!);
                }}
              >
                source
              </span>
            ) : null}
            <p>{run ? `${run.commandName} ${run.argv.join(" ")}` : entity.detail}</p>
            {run?.describe.length ? <p>{run.describe.join(" ")}</p> : null}
          </button>
        );
      })}
      {model.runs.length === 0 ? <p className="empty">No runs materialized in this trace.</p> : null}
    </div>
  );
}

function TraceDetail({ entity, onSourceSpan }: { entity: TraceEntity | null; onSourceSpan: (span: VixSpan) => void }) {
  if (!entity) {
    return <aside className="trace-detail"><p className="empty">No trace entity selected.</p></aside>;
  }
  return (
    <aside className="trace-detail">
      <div className="trace-detail-head">
        <span>{entity.kind}</span>
        <strong>{entity.label}</strong>
      </div>
      <p>{entity.detail}</p>
      <div className="trace-chip-row">
        {entity.chips.map((chip) => (
          <code key={chip}>{chip}</code>
        ))}
        {entity.startUs !== null ? <code>{formatTimestamp(entity.startUs)}</code> : null}
      </div>
      {entity.keyHash ? <code>key {shortHash(entity.keyHash)}</code> : null}
      {entity.run ? (
        <>
          <code>{entity.run.commandName} {entity.run.argv.join(" ")}</code>
          {entity.run.outputs.map((output) => (
            <code key={`${entity.id}-${output.path}`}>{output.path} · {shortHash(output.hash)}</code>
          ))}
        </>
      ) : null}
      {entity.observation ? <code>{entity.observation.key_text}</code> : null}
      {entity.span ? (
        <button type="button" className="btn" onClick={() => onSourceSpan(entity.span!)}>
          Source span
        </button>
      ) : null}
    </aside>
  );
}

function TraceEntityButton({
  entity,
  model,
  selectedId,
  hoveredId,
  onSelect,
  onHover,
  className,
  style,
}: TraceProjectionProps & { entity: TraceEntity; className: string; style?: CSSProperties }) {
  return (
    <button
      type="button"
      className={`${className} ${traceEntityClass(entity, model, selectedId, hoveredId)}`}
      style={style}
      onClick={() => onSelect(entity.id)}
      onMouseEnter={() => onHover(entity.id)}
      onMouseLeave={() => onHover(null)}
    >
      <span>{entity.label}</span>
      <code>{entity.detail}</code>
      {entity.chips.length ? <small>{entity.chips.join(" · ")}</small> : null}
    </button>
  );
}

function pendingOutputs(run: VixMachineRun): string[] {
  if (run.source_kind !== "merge-demand" || run.fn_name === "demo") {
    return [];
  }
  if (run.fn_name === "fallback") {
    return ["left.o"];
  }
  return ["left.o"];
}

function layoutTraceDag(model: TraceModel): {
  nodes: Map<string, { x: number; y: number; width: number; height: number }>;
  width: number;
  height: number;
} {
  const ranks = new Map<string, number>();
  for (const entity of model.entities.values()) {
    ranks.set(entity.id, entity.kind === "pending" ? 3 : Math.floor(entity.startIndex / 4));
  }
  for (let pass = 0; pass < model.entities.size; pass += 1) {
    let changed = false;
    for (const edge of model.edges) {
      const from = ranks.get(edge.from) ?? 0;
      const to = ranks.get(edge.to) ?? 0;
      if (to <= from) {
        ranks.set(edge.to, from + 1);
        changed = true;
      }
    }
    if (!changed) break;
  }

  const lanes = new Map<number, TraceEntity[]>();
  for (const entity of model.entities.values()) {
    const rank = ranks.get(entity.id) ?? 0;
    lanes.set(rank, [...(lanes.get(rank) ?? []), entity]);
  }
  for (const lane of lanes.values()) {
    lane.sort((left, right) => left.startIndex - right.startIndex || left.label.localeCompare(right.label));
  }

  const nodes = new Map<string, { x: number; y: number; width: number; height: number }>();
  let width = 0;
  let height = 0;
  const nodeWidth = 176;
  const nodeHeight = 62;
  const xGap = 72;
  const yGap = 24;
  for (const [rank, lane] of lanes) {
    const x = 24 + rank * (nodeWidth + xGap);
    width = Math.max(width, x + nodeWidth + 24);
    lane.forEach((entity, index) => {
      const y = 24 + index * (nodeHeight + yGap);
      height = Math.max(height, y + nodeHeight + 24);
      nodes.set(entity.id, { x, y, width: nodeWidth, height: nodeHeight });
    });
  }
  return { nodes, width: Math.max(width, 720), height: Math.max(height, 420) };
}

function timelineLanes(model: TraceModel): Array<{ name: string; entities: TraceEntity[] }> {
  const lanes = new Map<string, TraceEntity[]>();
  const push = (name: string, entity: TraceEntity) => lanes.set(name, [...(lanes.get(name) ?? []), entity]);
  for (const entity of model.invocations) {
    push(`fn ${entity.label}`, entity);
  }
  for (const entity of model.runs) {
    push(`cmd ${entity.run?.commandName ?? entity.label}`, entity);
  }
  for (const entity of model.observations) {
    push("observations", entity);
  }
  for (const entity of model.pending) {
    push("pending refs", entity);
  }
  return Array.from(lanes.entries()).map(([name, entities]) => ({
    name,
    entities: entities.sort((left, right) => left.startIndex - right.startIndex),
  }));
}

function timelineRange(model: TraceModel, entity: TraceEntity): [number, number] {
  const start = entity.startUs ?? eventIndexAsTime(model, entity.startIndex);
  const end = entity.endUs ?? eventIndexAsTime(model, entity.endIndex);
  const left = Math.max(0, Math.min(98, (start / model.timelineEndUs) * 100));
  const width = entity.startUs === entity.endUs || start === end
    ? 1.2
    : Math.max(1.2, ((end - start) / model.timelineEndUs) * 100);
  return [left, Math.min(width, 100 - left)];
}

function eventIndexAsTime(model: TraceModel, index: number): number {
  if (model.trace.length <= 1) return 0;
  return (Math.max(0, index) / (model.trace.length - 1)) * model.timelineEndUs;
}

function traceEntityClass(
  entity: TraceEntity,
  model: TraceModel,
  selectedId: string | null,
  hoveredId: string | null,
): string {
  const focusId = hoveredId ?? selectedId;
  const selected = selectedId === entity.id ? " selected" : "";
  const hovered = hoveredId === entity.id ? " hovered" : "";
  const related = focusId && focusId !== entity.id && entitiesRelated(model, focusId, entity.id) ? " related" : "";
  const dimmed = focusId && focusId !== entity.id && !related ? " dimmed" : "";
  return `trace-entity trace-${entity.visual}${selected}${hovered}${related}${dimmed}`;
}

function entitiesRelated(model: TraceModel, left: string, right: string): boolean {
  return model.edges.some((edge) => (edge.from === left && edge.to === right) || (edge.from === right && edge.to === left));
}

function entityAtSourceByte(model: TraceModel, byte: number): TraceEntity | null {
  return (
    Array.from(model.entities.values()).find((entity) => entity.span && entity.span.start <= byte && byte <= entity.span.end) ??
    null
  );
}

function machineResultSummary(run: VixMachineRun): string[] {
  if (run.result?.tree_entries.length) {
    return run.result.tree_entries.map((entry) => `${entry.path} = ${entry.contents}`);
  }
  if (run.result?.f64_value != null) {
    return [String(run.result.f64_value)];
  }
  return [run.result?.schema ?? "no result"];
}

function emptyRunSummary(
  event: Extract<VixDriveEvent, { type: "RunRequested" | "RunStarted" | "RunCompleted" }>,
  labels: Map<string, string>,
): MachineRunSummary {
  return {
    runId: event.run_id,
    command: event.command,
    commandName: event.command_name,
    output: event.output,
    outputLabel: labels.get(event.output) ?? event.output,
    argv: [],
    describe: [],
    span: null,
    requestedAt: null,
    startedAt: null,
    completedAt: null,
    serving: null,
    outputs: [],
  };
}

function runLifecycle(trace: VixDriveEvent[], labels: Map<string, string>): MachineRunSummary[] {
  const runs = new Map<number, MachineRunSummary>();
  const order: number[] = [];
  const ensure = (
    event: Extract<VixDriveEvent, { type: "RunRequested" | "RunStarted" | "RunCompleted" }>,
  ) => {
    const existing = runs.get(event.run_id);
    if (existing) {
      return existing;
    }
    order.push(event.run_id);
    const summary: MachineRunSummary = {
      runId: event.run_id,
      command: event.command,
      commandName: event.command_name,
      output: event.output,
      outputLabel: labels.get(event.output) ?? event.output,
      argv: [],
      describe: [],
      span: null,
      requestedAt: null,
      startedAt: null,
      completedAt: null,
      serving: null,
      outputs: [],
    };
    runs.set(event.run_id, summary);
    return summary;
  };

  for (const event of trace) {
    if (event.type === "RunRequested") {
      const summary = ensure(event);
      summary.requestedAt = event.timestamp_us;
      summary.argv = event.argv;
      summary.describe = event.describe;
      summary.span = event.span;
      summary.commandName = event.command_name;
    } else if (event.type === "RunStarted") {
      const summary = ensure(event);
      summary.startedAt = event.timestamp_us;
      summary.commandName = event.command_name;
    } else if (event.type === "RunCompleted") {
      const summary = ensure(event);
      summary.completedAt = event.timestamp_us;
      summary.commandName = event.command_name;
      summary.serving = event.serving;
      summary.outputs = event.outputs;
      summary.outputLabel = event.outputs[0]?.path ?? labels.get(event.output) ?? event.output;
    }
  }

  return order.map((id) => runs.get(id)!).filter(Boolean);
}

function formatDriveEvent(
  event: VixDriveEvent,
  fnLabels: Map<string, string>,
  runLabels: Map<string, string>,
): string {
  switch (event.type) {
    case "Demanded":
    case "MemoHit":
    case "Spawned":
    case "ParkedOn":
    case "Completed":
      return `${event.type} ${fnLabels.get(event.fn_hash) ?? event.fn_hash}`;
    case "SpawnedInvocation":
      return `${event.type} ${fnLabels.get(event.fn_hash) ?? event.fn_hash} ${event.key_hash.slice(0, 8)}`;
    case "StoreAlloc":
      return `${event.type} schema ${event.schema_ref.slice(0, 8)} ${event.deduped ? "deduped" : "new"}`;
    case "RunRequested":
      return `${event.type} #${event.run_id} ${event.command_name} -> ${runLabels.get(event.output) ?? event.output}`;
    case "RunStarted":
      return `${event.type} #${event.run_id} ${event.command_name} -> ${runLabels.get(event.output) ?? event.output}`;
    case "RunCompleted":
      return `${event.type} #${event.run_id} ${servingLabel(event.serving)} ${event.command_name} -> ${
        event.outputs.map((output) => output.path).join(", ") || runLabels.get(event.output) || event.output
      }`;
    case "Observation":
      return `Observation ${event.replayed ? "replayed" : "cold"} ${event.key_text || shortHash(event.key)}`;
  }
}

function runStatus(event: MachineRunSummary): string {
  if (event.completedAt !== null) return "completed";
  if (event.startedAt !== null) return "started";
  if (event.requestedAt !== null) return "requested";
  return "queued";
}

function runTimestamps(event: MachineRunSummary): string {
  const stamps = [
    event.requestedAt !== null ? `req ${formatTimestamp(event.requestedAt)}` : null,
    event.startedAt !== null ? `start ${formatTimestamp(event.startedAt)}` : null,
    event.completedAt !== null ? `done ${formatTimestamp(event.completedAt)}` : null,
  ].filter(Boolean);
  return stamps.join(" · ");
}

function servingLabel(serving: VixExecServing): string {
  if (serving.type === "Tier2Cutoff") {
    return `Tier2Cutoff ${serving.verified}`;
  }
  return serving.type;
}

function servingClass(serving: VixExecServing | null): TraceVisual {
  switch (serving?.type) {
    case "Ran":
      return "ran";
    case "Tier1Hit":
      return "tier1";
    case "Tier2Cutoff":
      return "tier2";
    case "Joined":
      return "joined";
    default:
      return "run";
  }
}

function eventTimestamp(event: VixDriveEvent): number | null {
  switch (event.type) {
    case "RunRequested":
    case "RunStarted":
    case "RunCompleted":
    case "Observation":
      return event.timestamp_us;
    default:
      return null;
  }
}

function formatTimestamp(timestampUs: number): string {
  if (timestampUs < 1000) {
    return `+${timestampUs} us`;
  }
  return `+${(timestampUs / 1000).toFixed(timestampUs >= 10_000 ? 1 : 2)} ms`;
}

function shortHash(hash: string): string {
  return hash.length > 12 ? hash.slice(0, 12) : hash;
}

function labelMap(labels: HashLabel[]): Map<string, string> {
  return new Map(labels.map((entry) => [entry.hash, entry.label]));
}

function PlanBody({ plan, parse }: { plan: PlanOutput; parse: ParseOutput | null }) {
  const parserStencilTotal = countPlanItems(plan.snark_stencils);
  const lexerStencilTotal = countPlanItems(plan.lexer_stencils);
  const totalStencilWork = parserStencilTotal + lexerStencilTotal;
  const dominant = plan.dominant_backend_execution;
  const dominantLexerExecution = parse?.dominant_lexer_stencil_execution ?? null;
  const dominantSnarkExecution = parse?.dominant_snark_stencil_execution ?? null;
  const backendExecutionItems = plan.backend_executions.map((summary) => ({
    ...summary,
    count: summary.total_count,
  }));

  return (
    <>
      <div className="plan-grid">
        <PlanFact
          label="Visibility"
          value={plan.fully_visible ? "full" : plan.parser_fully_visible || plan.lexer_fully_visible ? "partial" : "opaque"}
          detail={`parser ${plan.parser_fully_visible ? "visible" : "opaque"} · lexer ${plan.lexer_fully_visible ? "visible" : "opaque"}`}
        />
        <PlanFact
          label="Execution"
          value={plan.neutral_weavy_only ? "neutral" : "snark dialect"}
          detail={`${plan.neutral_weavy_op_count} neutral ops · ${plan.snark_intrinsic_count} intrinsics`}
        />
        <PlanFact
          label="Copy-patch"
          value={plan.copy_patch_jit_available ? "available" : "blocked"}
          detail={`${totalStencilWork} stencil sites · parser ${parserStencilTotal} · lexer ${lexerStencilTotal}`}
        />
      </div>

      {dominant ? (
        <div className="plan-row">
          <span>Dominant direct backend</span>
          <strong>{dominant.execution}</strong>
          <code>
            parser {dominant.parser_count} · lexer {dominant.lexer_count} · total {dominant.total_count}
          </code>
        </div>
      ) : null}

      {dominantSnarkExecution ? (
        <div className="plan-row">
          <span>Weavy parser hot lane</span>
          <strong>{dominantSnarkExecution.family}</strong>
          <code>
            {parse?.execution_lane ?? "Unknown"} · {dominantSnarkExecution.execution} ·{" "}
            {dominantSnarkExecution.count} executions ·{" "}
            {parse?.snark_intrinsic_count ?? 0} intrinsics
          </code>
        </div>
      ) : null}

      {dominantLexerExecution ? (
        <div className="plan-row">
          <span>Weavy lexer hot lane</span>
          <strong>{dominantLexerExecution.kind}</strong>
          <code>
            {dominantLexerExecution.count} executions · {parse?.lexer_call_count ?? 0} lex calls ·{" "}
            {parse?.lexer_direct_set_cache_hits ?? 0}/{parse?.lexer_direct_set_cache_misses ?? 0} cache hit/miss
          </code>
        </div>
      ) : null}

      <PlanTopList title="Parser stencil families" items={plan.snark_stencil_families} />
      <PlanTopList
        title="Weavy parser executions"
        items={(parse?.snark_stencil_executions ?? []).map((summary) => ({
          kind: `${summary.family} · ${summary.execution}`,
          count: summary.count,
        }))}
      />
      <PlanTopList
        title="Weavy lexer executions"
        items={(parse?.lexer_stencil_executions ?? []).map((summary) => ({
          kind: summary.kind,
          count: summary.count,
        }))}
      />
      <PlanTopList title="Lexer stencil ops" items={plan.lexer_stencils} />
      <PlanTopList title="Backend execution lanes" items={backendExecutionItems} />
      <PlanTopList title="Lowering barriers" items={plan.lowering_barriers} />
    </>
  );
}

function PlanFact({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="plan-fact">
      <span>{label}</span>
      <strong>{value}</strong>
      <code>{detail}</code>
    </div>
  );
}

function PlanTopList({ title, items }: { title: string; items: Array<{ count: number } & Record<string, unknown>> }) {
  if (!items.length) {
    return null;
  }
  return (
    <div className="plan-list">
      <h3>{title}</h3>
      {items.slice(0, 5).map((item, index) => (
        <div className="plan-list-row" key={`${title}-${index}-${planItemLabel(item)}`}>
          <code>{planItemLabel(item)}</code>
          <span>{item.count}</span>
        </div>
      ))}
    </div>
  );
}

function planItemLabel(item: Record<string, unknown>) {
  if (typeof item.kind === "string") {
    return item.kind;
  }
  if (typeof item.family === "string") {
    return item.family;
  }
  return typeof item.execution === "string" ? item.execution : String(item.kind ?? item.state ?? "unknown");
}

function countPlanItems(items: Array<{ count: number }>) {
  return items.reduce((total, item) => total + item.count, 0);
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

function vixMachineOptions(input: string): Array<{ name: string }> {
  if (input.includes("pub fn lua")) {
    return [{ name: "lua" }];
  }
  if (input.includes("pub fn selected") && input.includes("pub fn fallback")) {
    return [{ name: "selected" }, { name: "fallback" }, { name: "subtree_chain" }];
  }
  if (input.includes("pub fn demo() -> Float")) {
    return [{ name: "demo" }];
  }
  return [];
}

function defaultMachineFnForInput(input: string): string {
  return vixMachineOptions(input)[0]?.name ?? "";
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
