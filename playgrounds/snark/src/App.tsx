import { useMemo, useState } from "react";
import init, { parseBundle } from "@bearcove/snark-wasm";

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

type PlaygroundResponse = {
  ok: boolean;
  language: string | null;
  diagnostics: Diagnostic[];
  bundle: {
    grammar_path: string | null;
    grammar_js_path: string | null;
    query_paths: string[];
    corpus_paths: string[];
    generated_files_ignored: string[];
    scanner_paths: string[];
    active_scanner: string | null;
  };
  parse: ParseOutput | null;
  highlights: HighlightOutput[];
  corpus: CorpusOutput[];
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
  const passedCorpus = result?.corpus.filter((caseResult) => caseResult.passed).length ?? 0;
  const failedCorpus = result ? result.corpus.length - passedCorpus : 0;

  async function loadFiles(fileList: FileList | null) {
    if (!fileList || fileList.length === 0) {
      return;
    }
    const next = await Promise.all(
      Array.from(fileList).map(async (file) => ({
        path: normalizeBrowserPath(file),
        text: await file.text(),
      })),
    );
    setFiles(sortedFiles(next));
    setSelectedPath(next.some((file) => file.path === "src/grammar.json") ? "src/grammar.json" : next[0].path);
    setResult(null);
  }

  function updateSelectedFile(text: string) {
    setFiles((current) =>
      current.map((file) => (file.path === selectedFile.path ? { ...file, text } : file)),
    );
  }

  async function run() {
    setBusy(true);
    try {
      await wasmReady;
      const response = parseBundle(
        JSON.stringify({
          files,
          input,
          run_corpus: runCorpus,
        }),
      );
      setResult(JSON.parse(response) as PlaygroundResponse);
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
          <label className="check-row">
            <input
              type="checkbox"
              checked={runCorpus}
              onChange={(event) => setRunCorpus(event.currentTarget.checked)}
            />
            Corpus
          </label>
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
            <div className="capture-list">
              {result?.highlights.map((capture, index) => (
                <div className="capture-row" key={`${capture.capture_name}-${capture.start_byte}-${index}`}>
                  <span>@{capture.capture_name}</span>
                  <code>{capture.text}</code>
                  <small>
                    {capture.start_row}:{capture.start_column}-{capture.end_row}:{capture.end_column}
                  </small>
                </div>
              ))}
            </div>
          </section>
          <section>
            <h2>Corpus</h2>
            <div className="corpus-summary">
              <span>{passedCorpus} pass</span>
              <span>{failedCorpus} fail</span>
            </div>
            <div className="corpus-list">
              {result?.corpus.map((caseResult, index) => (
                <details key={`${caseResult.path}-${caseResult.case_name}-${index}`}>
                  <summary className={caseResult.passed ? "pass" : "fail"}>
                    {caseResult.case_name}
                  </summary>
                  <pre>{caseResult.error ?? caseResult.actual ?? ""}</pre>
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
  return (
    <div className="status-strip ok">
      <span>Accepted {result.parse?.accepted_count ?? 0}</span>
      <span>Failed {result.parse?.failure_count ?? 0}</span>
      <span>Live {result.parse?.max_live_versions ?? 0}</span>
      <span>Events {result.parse?.accepted_tree_event_count ?? 0}</span>
    </div>
  );
}

function normalizeBrowserPath(file: File) {
  const relative = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
  return (relative && relative.length > 0 ? relative : file.name).replace(/\\/g, "/");
}
