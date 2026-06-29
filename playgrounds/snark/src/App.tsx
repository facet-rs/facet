import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import init, { parseBundle } from "@bearcove/snark-wasm";
import { filesWithGrammarJson } from "./treeSitterDsl";

type BundleFile = {
  path: string;
  text: string;
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

function sortedFiles(files: BundleFile[]) {
  return [...files].sort((left, right) => left.path.localeCompare(right.path));
}

export function App() {
  const [files, setFiles] = useState<BundleFile[]>(defaultFiles);
  const [selectedPath, setSelectedPath] = useState("src/grammar.json");
  const [input, setInput] = useState("alpha 42 beta");
  const [runCorpus, setRunCorpus] = useState(false);
  const [result, setResult] = useState<PlaygroundResponse | null>(null);
  const [busy, setBusy] = useState(false);

  const selectedFile = useMemo(
    () => files.find((file) => file.path === selectedPath) ?? files[0],
    [files, selectedPath],
  );
  const sampleFiles = useMemo(
    () => sortedFiles(files).filter((file) => file.path.startsWith("samples/")),
    [files],
  );
  const highlightedSource = useMemo(
    () => renderHighlightedSource(input, result?.highlights ?? []),
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
    const next = sortedFiles(normalizeBrowserFiles(loaded));
    setFiles(next);
    setSelectedPath(
      next.some((file) => file.path === "src/grammar.json")
        ? "src/grammar.json"
        : next.some((file) => file.path === "grammar.js")
          ? "grammar.js"
          : next[0].path,
    );
    const firstSample = next.find((file) => file.path.startsWith("samples/"));
    if (firstSample) {
      setInput(firstSample.text);
    }
    setResult(null);
  }

  function updateSelectedFile(text: string) {
    setFiles((current) =>
      current.map((file) => (file.path === selectedFile.path ? { ...file, text } : file)),
    );
  }

  function exportResult() {
    if (!result) {
      return;
    }
    downloadJson("snark-playground-result.json", {
      request: {
        input,
        run_corpus: runCorpus,
        files: sortedFiles(files).map((file) => ({
          path: file.path,
          bytes: new TextEncoder().encode(file.text).length,
        })),
      },
      response: result,
    });
  }

  async function run() {
    setBusy(true);
    try {
      await wasmReady.catch((error: unknown) => {
        throw new PlaygroundRunError("wasm", errorMessage(error));
      });
      const runnableFiles = await filesWithGrammarJson(files).catch((error: unknown) => {
        throw new PlaygroundRunError("grammar.js", errorMessage(error));
      });
      const response = callParseBundle(runnableFiles, input, runCorpus);
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
            Load
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
            <button
              key={file.path}
              className={file.path === selectedFile.path ? "file-row active" : "file-row"}
              type="button"
              onClick={() => setSelectedPath(file.path)}
            >
              <span>{file.path}</span>
              <small>{file.text.length.toLocaleString()}b</small>
            </button>
          ))}
        </div>
        {result && (
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
        )}
      </section>

      <section className="pane work-pane" aria-label="Source and selected file">
        <div className="toolbar">
          <button type="button" onClick={() => void run()} disabled={busy}>
            {busy ? "Running" : "Run"}
          </button>
          <button type="button" onClick={exportResult} disabled={!result}>
            Export JSON
          </button>
          <label className="check-row">
            <input
              type="checkbox"
              checked={runCorpus}
              onChange={(event) => setRunCorpus(event.currentTarget.checked)}
            />
            Tests
          </label>
          <select
            aria-label="Load sample"
            className="sample-select"
            disabled={sampleFiles.length === 0}
            value=""
            onChange={(event) => {
              const sample = sampleFiles.find((file) => file.path === event.currentTarget.value);
              if (sample) {
                setInput(sample.text);
                setSelectedPath(sample.path);
              }
            }}
          >
            <option value="">Samples ({sampleFiles.length})</option>
            {sampleFiles.map((file) => (
              <option key={file.path} value={file.path}>
                {file.path}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="secondary"
            onClick={() => {
              setFiles(defaultFiles);
              setSelectedPath("src/grammar.json");
              setInput("alpha 42 beta");
              setResult(null);
            }}
          >
            Reset
          </button>
        </div>

        <div className="editor-grid">
          <label className="editor-block">
            <span>Source</span>
            <textarea value={input} onChange={(event) => setInput(event.currentTarget.value)} />
          </label>
          <label className="editor-block">
            <span>{selectedFile.path}</span>
            <textarea
              value={selectedFile.text}
              onChange={(event) => updateSelectedFile(event.currentTarget.value)}
            />
          </label>
        </div>
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
          <section>
            <h2>Tests</h2>
            <div className="corpus-summary">
              <span>{result?.tests.corpus_passed ?? 0} corpus pass</span>
              <span>{result?.tests.corpus_failed ?? 0} corpus fail</span>
              <span>{result?.tests.highlight_assertions_passed ?? 0} highlight pass</span>
              <span>
                {(result?.tests.highlight_assertions_failed ?? 0) +
                  (result?.tests.highlight_fixture_errors ?? 0)}{" "}
                highlight fail
              </span>
            </div>
            <div className="corpus-list">
              {result?.corpus.map((caseResult, index) => (
                <details key={`${caseResult.path}-${caseResult.case_name}-${index}`}>
                  <summary className={caseResult.passed ? "pass" : "fail"}>
                    {caseResult.case_name}
                  </summary>
                  <div className="test-actions">
                    <button type="button" onClick={() => setInput(caseResult.input)}>
                      Load input
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
              {result?.highlight_tests.map((fixture) => (
                <details key={fixture.path}>
                  <summary className={fixture.passed ? "pass" : "fail"}>
                    {fixture.path} ({fixture.passed_count}/{fixture.assertion_count})
                  </summary>
                  <div className="test-actions">
                    <button type="button" onClick={() => setInput(fixture.input)}>
                      Load fixture
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

function rawBrowserPath(file: File) {
  const relative = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
  return normalizePath(relative && relative.length > 0 ? relative : file.name);
}

function normalizeBrowserFiles(files: BundleFile[]) {
  const stripped = stripCommonRoot(files);
  return stripped.map((file) => ({ ...file, path: normalizeBundlePath(file.path) }));
}

function stripCommonRoot(files: BundleFile[]) {
  if (files.length === 0) {
    return files;
  }
  const firstSegments = files[0].path.split("/");
  if (firstSegments.length < 2) {
    return files;
  }
  const root = firstSegments[0];
  if (!files.every((file) => file.path === root || file.path.startsWith(`${root}/`))) {
    return files;
  }
  return files.map((file) => ({ ...file, path: file.path.slice(root.length + 1) }));
}

function normalizeBundlePath(path: string) {
  const normalized = normalizePath(path);
  const arborium = arboriumDefRelative(normalized);
  if (arborium) {
    const mapped = normalizeArboriumDefPath(arborium);
    if (mapped) {
      return mapped;
    }
  }
  return normalizePackagePath(normalized) ?? normalized;
}

function normalizePath(path: string) {
  let normalized = path.replace(/\\/g, "/");
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  return normalized;
}

function arboriumDefRelative(path: string) {
  if (path.startsWith("def/")) {
    return path.slice("def/".length);
  }
  const marker = "/def/";
  const index = path.indexOf(marker);
  return index >= 0 ? path.slice(index + marker.length) : null;
}

function normalizeArboriumDefPath(relative: string) {
  switch (relative) {
    case "grammar/grammar.js":
      return "grammar.js";
    case "grammar/grammar.json":
    case "grammar/src/grammar.json":
      return "src/grammar.json";
    case "grammar/scanner.c":
      return "src/scanner.c";
    case "grammar/scanner.cc":
      return "src/scanner.cc";
    case "grammar/src/parser.c":
      return "src/parser.c";
    case "grammar/src/parser.cc":
      return "src/parser.cc";
    case "grammar/src/parser.h":
      return "src/parser.h";
    case "grammar/src/node-types.json":
      return "src/node-types.json";
    case "grammar/bindings/node/binding.cc":
      return "bindings/node/binding.cc";
    default:
      break;
  }
  if (
    relative.startsWith("queries/") ||
    relative.startsWith("test/corpus/") ||
    relative.startsWith("test/highlight/") ||
    relative.startsWith("test/highlights/")
  ) {
    return relative;
  }
  if (relative.startsWith("samples/")) {
    return relative;
  }
  if (relative.startsWith("sample.")) {
    return `samples/${relative}`;
  }
  return null;
}

function normalizePackagePath(path: string) {
  if (
    [
      "grammar.js",
      "src/grammar.json",
      "src/scanner.c",
      "src/scanner.cc",
      "src/parser.c",
      "src/parser.cc",
      "src/parser.h",
      "src/node-types.json",
      "bindings/node/binding.cc",
    ].includes(path)
  ) {
    return path;
  }
  for (const suffix of [
    "/src/grammar.json",
    "/src/scanner.c",
    "/src/scanner.cc",
    "/src/parser.c",
    "/src/parser.cc",
    "/src/parser.h",
    "/src/node-types.json",
    "/bindings/node/binding.cc",
  ]) {
    if (path.endsWith(suffix)) {
      return suffix.slice(1);
    }
  }
  for (const token of ["/queries/", "/test/corpus/", "/test/highlight/", "/test/highlights/", "/samples/"]) {
    const index = path.indexOf(token);
    if (index >= 0) {
      return path.slice(index + 1);
    }
  }
  return null;
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

function callParseBundle(files: BundleFile[], input: string, runCorpus: boolean) {
  try {
    return parseBundle(
      JSON.stringify({
        files,
        input,
        run_corpus: runCorpus,
      }),
    );
  } catch (error) {
    throw new PlaygroundRunError("snark", errorMessage(error));
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function downloadJson(filename: string, value: unknown) {
  const blob = new Blob([JSON.stringify(value, null, 2) + "\n"], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
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
