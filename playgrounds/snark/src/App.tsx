import { useEffect, useMemo, useRef, useState } from "react";
import { runParse } from "./parseClient";
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
  parse: ParseOutput | null;
  highlights: HighlightOutput[];
  injections: InjectionOutput[];
  layers: LayerOutput[];
  corpus: CorpusOutput[];
  highlight_tests: HighlightTestOutput[];
  tests: TestSummary;
};

const defaultFiles: BundleFile[] = vendoredFiles;
const defaultGrammarRoot = defaultVendoredRootId;
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
  const [busyTask, setBusyTask] = useState<"parse" | "tests" | null>(null);
  const parseRequestId = useRef(0);
  const autoTestedKeyRef = useRef<string | null>(null);
  const bundledTestSnapshotRef = useRef<BundledTestSnapshot | null>(null);
  // The prepared session lives in the parse worker; here we only track which grammar it's
  // prepared for and the last input it parsed (for incremental-reparse gating).
  const preparedKeyRef = useRef<string | null>(null);
  const baselineInputRef = useRef<string | null>(null);
  const pendingSourceEditRef = useRef<PendingSourceEdit | null>(null);

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
    setBusyTask(runBundledTests ? "tests" : "parse");

    const timeout = window.setTimeout(() => {
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
    }, 150);

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

      // Only emit grammar.js -> grammar.json (in the DSL worker) when the bundle changed.
      let runnableFiles: BundleFile[] | null = null;
      if (needPrepare) {
        runnableFiles = await filesWithGrammarJson(files, activeGrammarRootId).catch(
          (error: unknown) => {
            throw new PlaygroundRunError("grammar.js", errorMessage(error));
          },
        );
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
  busyTask: "parse" | "tests" | null;
}) {
  if (busyTask) {
    return (
      <span className="pill busy">
        <span className="dot" />
        {busyTask === "tests" ? "Running tests" : "Parsing"}
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

      <details className="panel" open={!failure || Boolean(recovered)}>
        <summary>
          <span className="panel-title">S-expression</span>
          {result?.parse ? (
            <span className="panel-meta">
              {result.parse.accepted_count} accepted · {result.parse.failure_count} failed
              {result.parse.reuse_node_count ? ` · ${result.parse.reuse_node_count} reused` : ""}
              {result.parse.accepted_error_count || result.parse.accepted_missing_count
                ? ` · ${result.parse.accepted_error_count} ERROR · ${result.parse.accepted_missing_count} MISSING`
                : ""}
            </span>
          ) : null}
        </summary>
        <div className="panel-body">
          {sexp ? <pre className="sexp">{sexp}</pre> : <p className="empty">No parse tree.</p>}
        </div>
      </details>

      <details className="panel">
        <summary>
          <span className="panel-title">Captures</span>
          <span className="panel-meta">{captures.length}</span>
        </summary>
        <div className="panel-body">
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
        </div>
      </details>

      {layers.length ? (
        <details className="panel" open>
          <summary>
            <span className="panel-title">Layers</span>
            <span className="panel-meta">{countLayers(layers)}</span>
          </summary>
          <div className="panel-body">
            <LayerList layers={layers} />
          </div>
        </details>
      ) : null}

      {tests ? (
        <details className="panel" open>
          <summary>
            <span className="panel-title">Tests</span>
            <span className="panel-meta">
              {tests.tests.corpus_passed + tests.tests.highlight_assertions_passed} pass ·{" "}
              {tests.tests.corpus_failed +
                tests.tests.highlight_assertions_failed +
                tests.tests.highlight_fixture_errors}{" "}
              fail
            </span>
          </summary>
          <div className="panel-body">
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
          </div>
        </details>
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

  return (
    <details className="inventory">
      <summary>
        Bundle inventory
        <span className="inventory-counts">
          {queryPaths.length} queries · {corpusPaths.length} tests · {sourcePaths.length} source
        </span>
      </summary>
      <div className="inventory-body">
        <InventoryGroup title="Queries" paths={queryPaths} />
        <InventoryGroup title="Corpus & highlights" paths={corpusPaths} />
        <InventoryGroup title="Source inputs" paths={sourcePaths} />
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
