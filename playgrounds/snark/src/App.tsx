import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import init, { parseBundle } from "@bearcove/snark-wasm";
import {
  discoverGrammarRoots,
  filesWithGrammarJson,
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
  const [input, setInput] = useState("alpha 42 beta");
  const [result, setResult] = useState<PlaygroundResponse | null>(null);
  const [busy, setBusy] = useState(false);

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
  const highlightedSource = useMemo(
    () => (result ? renderHighlightedSource(input, result.highlights) : ""),
    [input, result?.highlights],
  );

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
    setFiles(next);
    setSelectedGrammarRoot(nextGrammarRoot);
    setInput("");
    setResult(null);
  }

  async function run() {
    setBusy(true);
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
      const response = callParseBundle(runnableFiles, input, hasBundledTests);
      setResult(JSON.parse(response) as PlaygroundResponse);
    } catch (error) {
      const diagnostic =
        error instanceof PlaygroundRunError
          ? { stage: error.stage, message: error.message }
          : { stage: "playground", message: errorMessage(error) };
      setResult(responseWithDiagnostic(diagnostic.stage, diagnostic.message, files));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <section className="pane bundle-pane" aria-label="Bundle files">
        <div className="pane-header">
          <div>
            <h1>Snark</h1>
            <p>{result?.language ?? "mini_playground"}</p>
          </div>
          <label className="file-button">
            Upload
            <input
              type="file"
              multiple
              {...({ webkitdirectory: "", directory: "" } as Record<string, string>)}
              onChange={(event) => void loadFiles(event.currentTarget.files)}
            />
          </label>
        </div>
        <div className="file-list">
          {sortedFiles(files).map((file) => (
            <div
              key={file.path}
              className="file-row"
            >
              <span>{file.path}</span>
              <small>{file.text.length.toLocaleString()}b</small>
            </div>
          ))}
        </div>
        {!result && (
          <LocalBundleInventory
            files={projectedFiles}
            grammarRootLabel={activeGrammarRoot?.label ?? "bundle root"}
          />
        )}
        {result && (
          <BundleInventory result={result} />
        )}
      </section>

      <section className="pane work-pane" aria-label="Source">
        <div className="toolbar">
          <button type="button" onClick={() => void run()} disabled={busy}>
            {busy ? "Running" : "Run"}
          </button>
          {grammarRoots.length > 1 ? (
            <select
              aria-label="Grammar root"
              className="grammar-select"
              value={activeGrammarRoot?.id ?? ""}
              onChange={(event) => {
                setSelectedGrammarRoot(event.currentTarget.value);
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
              className="sample-select"
              value=""
              onChange={(event) => {
                const sample = sampleFiles.find((file) => file.sourcePath === event.currentTarget.value);
                if (sample) {
                  setInput(sample.text);
                }
              }}
            >
              <option value="">Samples ({sampleFiles.length})</option>
              {sampleFiles.map((file) => (
                <option key={file.sourcePath} value={file.sourcePath}>
                  {file.path}
                </option>
              ))}
            </select>
          ) : null}
        </div>

        <label className="editor-block source-editor">
          <span>Source</span>
          <textarea value={input} onChange={(event) => setInput(event.currentTarget.value)} />
        </label>
      </section>

      <section className="pane result-pane" aria-label="Parse results">
        <StatusStrip result={result} />
        {result?.diagnostics.length ? (
          <div className="diagnostics">
            {result.diagnostics.map((diagnostic, index) => (
              <div className="diagnostic" key={`${diagnostic.stage}-${index}`}>
                <strong>{diagnostic.stage}</strong>
                <span>{diagnostic.message}</span>
              </div>
            ))}
          </div>
        ) : null}

        <div className="result-tabs">
          <section>
            <h2>S-expression</h2>
            <pre>{result?.parse?.sexp ?? ""}</pre>
          </section>
          <section>
            <h2>Highlights</h2>
            <pre className="highlighted-source">{highlightedSource}</pre>
            <div className="capture-list">
              {result?.highlights.map((capture, index) => (
                <div className="capture-row" key={`${capture.capture_name}-${capture.start_byte}-${index}`}>
                  <span className={`capture-chip ${captureClass(capture.capture_name)}`}>
                    @{capture.capture_name}
                  </span>
                  <code>{capture.text}</code>
                  <small>
                    {capture.start_row}:{capture.start_column}-{capture.end_row}:{capture.end_column}
                  </small>
                </div>
              ))}
            </div>
          </section>
          {result?.tests.requested ? (
            <section>
              <h2>Tests</h2>
              <div className="corpus-summary">
                <span>{result.tests.corpus_passed} corpus pass</span>
                <span>{result.tests.corpus_failed} corpus fail</span>
                <span>{result.tests.highlight_assertions_passed} highlight pass</span>
                <span>
                  {result.tests.highlight_assertions_failed + result.tests.highlight_fixture_errors}{" "}
                  highlight fail
                </span>
              </div>
              <div className="corpus-list">
                {result.corpus.map((caseResult, index) => (
                  <details key={`${caseResult.path}-${caseResult.case_name}-${index}`}>
                    <summary className={caseResult.passed ? "pass" : "fail"}>
                      {caseResult.case_name}
                    </summary>
                    <div className="test-actions">
                      <button type="button" onClick={() => setInput(caseResult.input)}>
                        Use input
                      </button>
                    </div>
                    <div className="test-detail-grid">
                      {caseResult.error ? (
                        <section>
                          <h3>Error</h3>
                          <pre>{caseResult.error}</pre>
                        </section>
                      ) : null}
                      <section>
                        <h3>Input</h3>
                        <pre>{caseResult.input}</pre>
                      </section>
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
                {result.highlight_tests.map((fixture) => (
                  <details key={fixture.path}>
                    <summary className={fixture.passed ? "pass" : "fail"}>
                      {fixture.path} ({fixture.passed_count}/{fixture.assertion_count})
                    </summary>
                    <div className="test-actions">
                      <button type="button" onClick={() => setInput(fixture.input)}>
                        Use fixture
                      </button>
                    </div>
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
                            <small>
                              {assertion.row}:{assertion.column}+{assertion.length}
                            </small>
                            {assertion.message ? <code>{assertion.message}</code> : null}
                            {assertion.observed_captures.length ? (
                              <code>{assertion.observed_captures.join(", ")}</code>
                            ) : null}
                          </div>
                        ))}
                      </div>
                    )}
                  </details>
                ))}
              </div>
            </section>
          ) : null}
        </div>
        {result?.limitations.length ? (
          <ul className="limitations">
            {result.limitations.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        ) : null}
      </section>
    </main>
  );
}

function StatusStrip({ result }: { result: PlaygroundResponse | null }) {
  if (!result) {
    return <div className="status-strip idle">Ready</div>;
  }
  if (!result.ok) {
    return <div className="status-strip error">Parse failed</div>;
  }
  const corpusFailures = result.tests.corpus_failed;
  const highlightFailures =
    result.tests.highlight_assertions_failed + result.tests.highlight_fixture_errors;
  const testFailures = corpusFailures + highlightFailures;
  if (testFailures > 0) {
    return (
      <div className="status-strip warn">
        <span>Parse accepted {result.parse?.accepted_count ?? 0}</span>
        <span>{corpusFailures} corpus fail</span>
        <span>{highlightFailures} highlight fail</span>
      </div>
    );
  }
  return (
    <div className="status-strip ok">
      <span>Accepted {result.parse?.accepted_count ?? 0}</span>
      <span>Failed {result.parse?.failure_count ?? 0}</span>
      <span>Live {result.parse?.max_live_versions ?? 0}</span>
      <span>Events {result.parse?.accepted_tree_event_count ?? 0}</span>
    </div>
  );
}

function BundleInventory({ result }: { result: PlaygroundResponse }) {
  return (
    <div className="bundle-inventory">
      <dl className="bundle-facts">
        <div>
          <dt>grammar</dt>
          <dd>{result.bundle.grammar_path ?? "missing"}</dd>
        </div>
        <div>
          <dt>grammar.js</dt>
          <dd>{result.bundle.grammar_js_path ?? "none"}</dd>
        </div>
        <div>
          <dt>queries</dt>
          <dd>{result.bundle.query_paths.length}</dd>
        </div>
        <div>
          <dt>corpus</dt>
          <dd>{result.bundle.corpus_paths.length}</dd>
        </div>
        <div>
          <dt>samples</dt>
          <dd>{result.bundle.sample_paths.length}</dd>
        </div>
        <div>
          <dt>ignored</dt>
          <dd>{result.bundle.generated_files_ignored.length}</dd>
        </div>
        <div>
          <dt>scanner</dt>
          <dd>{result.bundle.active_scanner ? "active" : result.bundle.scanner_paths.length}</dd>
        </div>
      </dl>
      <details className="bundle-paths">
        <summary>Bundle inventory</summary>
        <BundlePathList title="Queries" paths={result.bundle.query_paths} />
        <BundlePathList title="Corpus and highlights" paths={result.bundle.corpus_paths} />
        <BundlePathList title="Samples" paths={result.bundle.sample_paths} />
        <BundlePathList title="Scanners" paths={result.bundle.scanner_paths} />
        <BundlePathList title="Ignored generated files" paths={result.bundle.generated_files_ignored} />
        {result.bundle.active_scanner ? (
          <div className="bundle-path-group">
            <h3>Active scanner</h3>
            <code>{result.bundle.active_scanner}</code>
          </div>
        ) : null}
      </details>
    </div>
  );
}

function LocalBundleInventory({
  files,
  grammarRootLabel,
}: {
  files: { path: string; sourcePath: string }[];
  grammarRootLabel: string;
}) {
  const grammarPath =
    files.find((file) => file.path === "src/grammar.json")?.path ??
    files.find((file) => file.path === "grammar.js")?.path ??
    "missing";
  const queryCount = files.filter((file) => file.path.startsWith("queries/")).length;
  const corpusCount = files.filter(
    (file) =>
      file.path.startsWith("test/corpus/") ||
      file.path.startsWith("test/highlight/") ||
      file.path.startsWith("test/highlights/"),
  ).length;
  const sampleCount = files.filter((file) => file.path.startsWith("samples/")).length;
  const scannerCount = files.filter(
    (file) => file.path === "src/scanner.c" || file.path === "src/scanner.cc",
  ).length;
  const ignoredCount = files.filter((file) => isGeneratedPath(file.path)).length;

  return (
    <div className="bundle-inventory">
      <dl className="bundle-facts">
        <div>
          <dt>root</dt>
          <dd>{grammarRootLabel}</dd>
        </div>
        <div>
          <dt>grammar</dt>
          <dd>{grammarPath}</dd>
        </div>
        <div>
          <dt>queries</dt>
          <dd>{queryCount}</dd>
        </div>
        <div>
          <dt>corpus</dt>
          <dd>{corpusCount}</dd>
        </div>
        <div>
          <dt>samples</dt>
          <dd>{sampleCount}</dd>
        </div>
        <div>
          <dt>ignored</dt>
          <dd>{ignoredCount}</dd>
        </div>
        <div>
          <dt>scanner</dt>
          <dd>{scannerCount}</dd>
        </div>
      </dl>
    </div>
  );
}

function BundlePathList({ title, paths }: { title: string; paths: string[] }) {
  return (
    <div className="bundle-path-group">
      <h3>{title}</h3>
      {paths.length ? (
        <ul>
          {paths.map((path) => (
            <li key={path}>
              <code>{path}</code>
            </li>
          ))}
        </ul>
      ) : (
        <p>none</p>
      )}
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
    diagnostics: [{ stage, message }],
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
    limitations: ["Tree-sitter grammar.js is evaluated in a browser Worker before Snark receives src/grammar.json."],
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
        title={`@${capture.capture.capture_name}`}
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
