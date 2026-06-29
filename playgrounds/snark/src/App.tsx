import type { ReactNode } from "react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import init, { parseBundle } from "@bearcove/snark-wasm";
import {
  discoverGrammarRoots,
  filesWithGrammarJson,
  firstSampleForGrammarRootId,
  grammarRootForId,
  normalizeBundleFiles,
  preferredGrammarRootId,
  projectedFilesForGrammarRootId,
  sortedFiles,
  normalizePath,
  type DslBundleFile,
} from "./treeSitterDsl";

type BundleFile = DslBundleFile;

type SampleFile = BundleFile & {
  sourcePath: string;
};

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
  accepted_tree_event_count: number;
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
  corpus: CorpusOutput[];
  highlight_tests: HighlightTestOutput[];
  tests: TestSummary;
  limitations: string[];
};

const wasmReady = init();

const defaultFiles: BundleFile[] = [
  {
    path: "src/grammar.json",
    text: JSON.stringify(
      {
        name: "mini_playground",
        rules: {
          document: {
            type: "REPEAT1",
            content: { type: "SYMBOL", name: "item" },
          },
          item: {
            type: "CHOICE",
            members: [
              { type: "SYMBOL", name: "identifier" },
              { type: "SYMBOL", name: "number" },
            ],
          },
          identifier: {
            type: "PATTERN",
            value: "[A-Za-z_][A-Za-z0-9_]*",
          },
          number: {
            type: "PATTERN",
            value: "\\d+",
          },
        },
        extras: [{ type: "PATTERN", value: "\\s" }],
      },
      null,
      2,
    ),
  },
  {
    path: "queries/highlights.scm",
    text: "(identifier) @variable\n(number) @number\n",
  },
  {
    path: "test/corpus/main.txt",
    text: [
      "==================",
      "Words and numbers",
      "==================",
      "",
      "alpha 42 beta",
      "",
      "------------------",
      "",
      "(document (item (identifier)) (item (number)) (item (identifier)))",
      "",
    ].join("\n"),
  },
];

export function App() {
  const [files, setFiles] = useState<BundleFile[]>(defaultFiles);
  const [selectedGrammarRoot, setSelectedGrammarRoot] = useState("");
  const [selectedSamplePath, setSelectedSamplePath] = useState("");
  const [input, setInput] = useState("alpha 42 beta");
  const [result, setResult] = useState<PlaygroundResponse | null>(null);
  const [busyTask, setBusyTask] = useState<"parse" | "tests" | null>(null);
  const [editorScroll, setEditorScroll] = useState({ left: 0, top: 0 });
  const parseRequestId = useRef(0);

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
  const sampleFiles = useMemo(
    () => sortedFiles(projectedFiles).filter((file): file is SampleFile => file.path.startsWith("samples/")),
    [projectedFiles],
  );
  const visibleBundleFiles = useMemo(
    () => sortedFiles(projectedFiles).filter((file) => isRuntimeBundlePath(file.path)),
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

  useEffect(() => {
    const requestId = parseRequestId.current + 1;
    parseRequestId.current = requestId;
    setBusyTask("parse");

    const timeout = window.setTimeout(() => {
      void playgroundResponse(false)
        .then((response) => {
          if (parseRequestId.current === requestId) {
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
  }, [activeGrammarRootId, files, input, projectedFiles]);

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
    const next = sortedFiles(normalizeBundleFiles(loaded));
    const nextGrammarRoot = preferredGrammarRootId(next);
    const nextSample = firstSampleForGrammarRootId(next, nextGrammarRoot);
    setFiles(next);
    setSelectedGrammarRoot(nextGrammarRoot);
    setSelectedSamplePath(nextSample?.sourcePath ?? "");
    setInput(nextSample?.text ?? "");
    setResult(null);
  }

  function updateSourceInput(nextInput: string, samplePath = "") {
    setInput(nextInput);
    setSelectedSamplePath(samplePath);
    setResult(null);
  }

  async function playgroundResponse(runBundledTests: boolean): Promise<PlaygroundResponse> {
    try {
      await wasmReady.catch((error: unknown) => {
        throw new PlaygroundRunError("wasm", errorMessage(error));
      });
      const runnableFiles = await filesWithGrammarJson(
        files,
        activeGrammarRootId,
      ).catch((error: unknown) => {
        throw new PlaygroundRunError("grammar.js", errorMessage(error));
      });
      const response = callParseBundle(runnableFiles, input, runBundledTests);
      return JSON.parse(response) as PlaygroundResponse;
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
    const response = await playgroundResponse(true);
    if (parseRequestId.current === requestId) {
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
                className={file.sourcePath === selectedSamplePath ? "file-row active" : "file-row"}
                title={file.sourcePath === file.path ? file.path : `${file.path} from ${file.sourcePath}`}
                onClick={() => updateSourceInput(file.text, file.sourcePath)}
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

        <BundleInventory result={result} files={projectedFiles} />
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
                  const nextSample = firstSampleForGrammarRootId(files, nextGrammarRoot);
                  setSelectedGrammarRoot(nextGrammarRoot);
                  setSelectedSamplePath(nextSample?.sourcePath ?? "");
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
            {sampleFiles.length > 0 ? (
              <select
                aria-label="Sample"
                className="select"
                value={selectedSamplePath}
                onChange={(event) => {
                  const sample = sampleFiles.find((file) => file.sourcePath === event.currentTarget.value);
                  if (sample) {
                    updateSourceInput(sample.text, sample.sourcePath);
                  } else {
                    setSelectedSamplePath("");
                  }
                }}
              >
                <option value="">Samples · {sampleFiles.length}</option>
                {sampleFiles.map((file) => (
                  <option key={file.sourcePath} value={file.sourcePath}>
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

        <Editor
          input={input}
          result={result}
          scroll={editorScroll}
          onChange={(value) => updateSourceInput(value)}
          onScroll={(left, top) => setEditorScroll({ left, top })}
        />

        <ResultsDock result={result} onUseInput={(value) => updateSourceInput(value)} />
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
    return (
      <span className="pill error">
        <span className="dot" />
        Parse failed
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

const LINT_HEIGHT = 84;

function Editor({
  input,
  result,
  scroll,
  onChange,
  onScroll,
}: {
  input: string;
  result: PlaygroundResponse | null;
  scroll: { left: number; top: number };
  onChange: (value: string) => void;
  onScroll: (left: number, top: number) => void;
}) {
  const diagnostic = placedDiagnostic(result);
  const span = diagnostic?.primary_span ?? null;
  const errorLine = span ? span.start_row : null;
  const lines = useMemo(() => input.split("\n"), [input]);
  const highlightNodes = useMemo(() => renderSourceLayer(input, result), [input, result]);
  const hasInlineLint = diagnostic !== null && span !== null && errorLine !== null;

  const gutter: ReactNode[] = [];
  for (let i = 0; i < lines.length; i += 1) {
    gutter.push(
      <span className={i === errorLine ? "ln err" : "ln"} key={`ln-${i}`}>
        {i + 1}
      </span>,
    );
    if (i === errorLine) {
      gutter.push(<span className="ln-spacer" key="ln-spacer" aria-hidden="true" />);
    }
  }

  let codeContent: ReactNode;
  if (hasInlineLint && span && errorLine !== null && diagnostic) {
    codeContent = lines.map((lineText, i) => {
      if (i !== errorLine) {
        return (
          <span className="src-line" key={`l-${i}`}>
            {lineText}
            {i < lines.length - 1 ? "\n" : ""}
          </span>
        );
      }
      const c0 = Math.min(span.start_column, lineText.length);
      const c1 = span.end_row === span.start_row ? Math.min(span.end_column, lineText.length) : lineText.length;
      const token = lineText.slice(c0, Math.max(c0, c1)) || " ";
      return (
        <Fragment key={`l-${i}`}>
          <span className="src-line err-line">
            {lineText.slice(0, c0)}
            <span className="err-token">{token}</span>
            {lineText.slice(Math.max(c0, c1))}
            {"\n"}
          </span>
          <span className="code-lint" style={{ height: LINT_HEIGHT }}>
            <span className="code-lint-head">
              <span className="code-lint-stage">{diagnostic.stage}</span>
              <span className="code-lint-loc">
                {span.start_row + 1}:{span.start_column + 1}
              </span>
            </span>
            <span className="code-lint-body">{diagnostic.message}</span>
          </span>
        </Fragment>
      );
    });
  } else if (highlightNodes) {
    codeContent = highlightNodes;
  } else {
    codeContent = input;
  }

  return (
    <div className="editor">
      <div className="gutter-viewport">
        <div className="gutter" style={{ transform: `translateY(${-scroll.top}px)` }}>
          {gutter}
        </div>
      </div>
      <div className="code-viewport">
        <pre
          aria-hidden="true"
          className="code-layer"
          style={{ transform: `translate(${-scroll.left}px, ${-scroll.top}px)` }}
        >
          {codeContent}
        </pre>
        <textarea
          className={hasInlineLint ? "with-lint" : ""}
          value={input}
          spellCheck={false}
          wrap="off"
          onChange={(event) => onChange(event.currentTarget.value)}
          onScroll={(event) => onScroll(event.currentTarget.scrollLeft, event.currentTarget.scrollTop)}
        />
      </div>
    </div>
  );
}

function ResultsDock({
  result,
  onUseInput,
}: {
  result: PlaygroundResponse | null;
  onUseInput: (value: string) => void;
}) {
  const failure = result && !result.ok ? result : null;
  const sexp = result?.parse?.sexp ?? "";
  const captures = result?.highlights ?? [];
  const tests = result?.tests.requested ? result : null;
  const unplaced = result?.diagnostics.filter((diagnostic) => !diagnostic.primary_span) ?? [];

  return (
    <div className="dock">
      {failure ? (
        <div className="dock-failure">
          <strong>Parse failed</strong>
          {unplaced.map((diagnostic, index) => (
            <div className="dock-failure-row" key={`${diagnostic.stage}-${index}`}>
              <span className="dock-failure-stage">{diagnostic.stage}</span>
              <code>{diagnostic.message}</code>
            </div>
          ))}
        </div>
      ) : null}

      <details className="panel" open={!failure}>
        <summary>
          <span className="panel-title">S-expression</span>
          {result?.parse ? (
            <span className="panel-meta">
              {result.parse.accepted_count} accepted · {result.parse.failure_count} failed
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
                        onUseInput(caseResult.input);
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
                        onUseInput(fixture.input);
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

function BundleInventory({
  result,
  files,
}: {
  result: PlaygroundResponse | null;
  files: { path: string; sourcePath: string }[];
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
  const samplePaths = result?.bundle.sample_paths ?? files.filter((file) => file.path.startsWith("samples/")).map((file) => file.path);
  const scannerPaths = result?.bundle.scanner_paths ?? [];
  const ignoredPaths =
    result?.bundle.generated_files_ignored ?? files.filter((file) => isGeneratedPath(file.path)).map((file) => file.path);

  return (
    <details className="inventory">
      <summary>
        Bundle inventory
        <span className="inventory-counts">
          {queryPaths.length}q · {corpusPaths.length}c · {samplePaths.length}s
        </span>
      </summary>
      <div className="inventory-body">
        <InventoryGroup title="Queries" paths={queryPaths} />
        <InventoryGroup title="Corpus & highlights" paths={corpusPaths} />
        <InventoryGroup title="Samples" paths={samplePaths} />
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
    limitations: [],
  };
}

class PlaygroundRunError extends Error {
  readonly stage: string;

  constructor(stage: string, message: string) {
    super(message);
    this.stage = stage;
  }
}

function callParseBundle(files: BundleFile[], input: string, runBundledTests: boolean) {
  try {
    return parseBundle(
      JSON.stringify({
        files,
        input,
        run_corpus: runBundledTests,
      }),
    );
  } catch (error) {
    throw new PlaygroundRunError("snark", errorMessage(error));
  }
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

function isRuntimeBundlePath(path: string) {
  return (
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

function renderHighlightedSource(input: string, captures: HighlightOutput[]) {
  if (input.length === 0) {
    return "";
  }
  const byteToStringIndex = byteOffsetMap(input);
  const selected = nonOverlappingCaptures(captures, byteToStringIndex, input.length);
  if (selected.length === 0) {
    return input;
  }

  const nodes: ReactNode[] = [];
  let cursor = 0;
  for (const capture of selected) {
    if (capture.startIndex > cursor) {
      nodes.push(input.slice(cursor, capture.startIndex));
    }
    nodes.push(
      <span
        className={`source-capture ${captureClass(capture.capture.capture_name)}`}
        key={`${capture.capture.capture_name}-${capture.capture.start_byte}-${capture.capture.end_byte}`}
      >
        {input.slice(capture.startIndex, capture.endIndex)}
      </span>,
    );
    cursor = capture.endIndex;
  }
  if (cursor < input.length) {
    nodes.push(input.slice(cursor));
  }
  return nodes;
}

function renderSourceLayer(input: string, result: PlaygroundResponse | null) {
  if (!result || !result.ok) {
    return null;
  }
  if (result.highlights.length === 0) {
    return null;
  }
  return renderHighlightedSource(input, result.highlights);
}

function placedDiagnostic(result: PlaygroundResponse | null) {
  return result?.diagnostics.find((candidate) => candidate.primary_span) ?? null;
}

function nonOverlappingCaptures(
  captures: HighlightOutput[],
  byteToStringIndex: number[],
  inputLength: number,
) {
  return captures
    .map((capture) => ({
      capture,
      startIndex: byteToStringIndex[capture.start_byte] ?? inputLength,
      endIndex: byteToStringIndex[capture.end_byte] ?? inputLength,
    }))
    .filter((capture) => capture.startIndex < capture.endIndex)
    .sort((left, right) => {
      if (left.startIndex !== right.startIndex) {
        return left.startIndex - right.startIndex;
      }
      if (left.endIndex !== right.endIndex) {
        return right.endIndex - left.endIndex;
      }
      return left.capture.capture_name.localeCompare(right.capture.capture_name);
    })
    .reduce<Array<{ capture: HighlightOutput; startIndex: number; endIndex: number }>>(
      (selected, capture) => {
        const previous = selected[selected.length - 1];
        if (!previous || capture.startIndex >= previous.endIndex) {
          selected.push(capture);
        }
        return selected;
      },
      [],
    );
}

function byteOffsetMap(input: string) {
  const encoder = new TextEncoder();
  const totalBytes = encoder.encode(input).length;
  const map = new Array<number>(totalBytes + 1);
  let byteOffset = 0;
  let stringIndex = 0;
  map[0] = 0;
  for (const char of input) {
    const nextByteOffset = byteOffset + encoder.encode(char).length;
    const nextStringIndex = stringIndex + char.length;
    for (let byte = byteOffset; byte < nextByteOffset; byte += 1) {
      map[byte] = stringIndex;
    }
    map[nextByteOffset] = nextStringIndex;
    byteOffset = nextByteOffset;
    stringIndex = nextStringIndex;
  }
  return map;
}

function captureClass(captureName: string) {
  const first = captureName.split(".")[0] ?? captureName;
  switch (first) {
    case "attribute":
    case "property":
      return "capture-property";
    case "comment":
      return "capture-comment";
    case "constant":
    case "number":
      return "capture-number";
    case "function":
    case "method":
      return "capture-function";
    case "keyword":
    case "operator":
      return "capture-keyword";
    case "punctuation":
      return "capture-punctuation";
    case "string":
      return "capture-string";
    case "type":
      return "capture-type";
    case "variable":
      return "capture-variable";
    default:
      return "capture-default";
  }
}
