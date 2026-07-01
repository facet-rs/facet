// One-click parse-throughput benchmark for the playground.
//
// Runs the active grammar's samples in ascending size order, reusing one prepared
// session, and records the wasm "run parser" time (from the response's per-phase
// timings) plus the JS round-trip wall time for each. The result is rendered as a
// chart + table AND assigned to `window.__snarkBenchResult`, and the whole run can
// be triggered headlessly via `window.__snarkRunBenchmark()` — so an automated
// browser (Playwright, or the agent) can drive it and read the numbers back.

type PhaseTiming = { name: string; ms: number };
type Timings = { prepare: PhaseTiming[]; parse: PhaseTiming | null };
type ParseResponseLike = { timings?: Timings; ok?: boolean };

export type BenchSample = {
  name: string;
  bytes: number;
  /** Min wasm parse time (the "run parser" phase) over the runs. */
  parseMs: number;
  /** Min JS round-trip time (worker post + parse + JSON) over the runs. */
  wallMs: number;
  bytesPerMs: number;
  /** parseMs ratio vs the previous (smaller) sample. ~= the size ratio when linear. */
  xPrev: number;
  /** bytes ratio vs the previous sample, for comparison against xPrev. */
  sizeRatio: number;
};

export type BenchReport = {
  grammar: string;
  generatedAt: string;
  runsPerSample: number;
  prepare: PhaseTiming[];
  samples: BenchSample[];
  /** Median of (xPrev / sizeRatio) across the ladder. ~1.0 = linear, >1 = super-linear. */
  scalingIndex: number;
};

declare global {
  interface Window {
    __snarkRunBenchmark?: () => Promise<BenchReport>;
    __snarkBenchResult?: BenchReport;
  }
}

export async function runBenchmark(opts: {
  grammar: string;
  samples: { name: string; text: string }[];
  runsPerSample?: number;
  parse: (input: string) => Promise<ParseResponseLike>;
  onProgress?: (done: number, total: number, name: string) => void;
}): Promise<BenchReport> {
  const runs = opts.runsPerSample ?? 3;
  const sorted = [...opts.samples].sort((a, b) => a.text.length - b.text.length);
  const encoder = new TextEncoder();

  // Warm-up: this also (re)prepares the session, so the prepare cost never lands on
  // a measured run and we can read the one-time prepare phase timings off it.
  const warm = sorted.length ? await opts.parse(sorted[0].text) : null;
  const prepare = warm?.timings?.prepare ?? [];

  const samples: BenchSample[] = [];
  let prevParseMs: number | null = null;
  let prevBytes: number | null = null;
  for (let s = 0; s < sorted.length; s += 1) {
    const sample = sorted[s];
    const bytes = encoder.encode(sample.text).length;
    let bestParse = Infinity;
    let bestWall = Infinity;
    for (let r = 0; r < runs; r += 1) {
      const t0 = performance.now();
      const resp = await opts.parse(sample.text);
      const wall = performance.now() - t0;
      const pm = resp.timings?.parse?.ms;
      if (typeof pm === "number" && Number.isFinite(pm)) bestParse = Math.min(bestParse, pm);
      bestWall = Math.min(bestWall, wall);
    }
    if (!Number.isFinite(bestParse)) bestParse = 0;
    const sizeRatio = prevBytes && prevBytes > 0 ? bytes / prevBytes : 0;
    const xPrev = prevParseMs && prevParseMs > 0 ? bestParse / prevParseMs : 0;
    samples.push({
      name: sample.name,
      bytes,
      parseMs: bestParse,
      wallMs: Number.isFinite(bestWall) ? bestWall : 0,
      bytesPerMs: bestParse > 0 ? bytes / bestParse : 0,
      xPrev,
      sizeRatio,
    });
    prevParseMs = bestParse;
    prevBytes = bytes;
    opts.onProgress?.(s + 1, sorted.length, sample.name);
  }

  const ratios = samples
    .filter((row) => row.sizeRatio > 0 && row.xPrev > 0)
    .map((row) => row.xPrev / row.sizeRatio)
    .sort((a, b) => a - b);
  const scalingIndex = ratios.length ? ratios[Math.floor(ratios.length / 2)] : 1;

  return {
    grammar: opts.grammar,
    generatedAt: new Date().toISOString(),
    runsPerSample: runs,
    prepare,
    samples,
    scalingIndex,
  };
}

// ---------------------------------------------------------------------------
// Rendering

function fmtBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function fmtMs(ms: number): string {
  return ms >= 1 ? `${ms.toFixed(2)} ms` : `${ms.toFixed(3)} ms`;
}

/** parse-ms vs bytes scatter+line. A straight line from the origin == linear scaling. */
function ScalingChart({ samples }: { samples: BenchSample[] }) {
  const width = 520;
  const height = 220;
  const pad = { left: 52, right: 14, top: 14, bottom: 34 };
  const plotW = width - pad.left - pad.right;
  const plotH = height - pad.top - pad.bottom;
  const maxBytes = Math.max(1, ...samples.map((s) => s.bytes));
  const maxMs = Math.max(1e-6, ...samples.map((s) => Math.max(s.parseMs, s.wallMs)));
  const x = (bytes: number) => pad.left + (bytes / maxBytes) * plotW;
  const y = (ms: number) => pad.top + plotH - (ms / maxMs) * plotH;

  const line = (key: keyof BenchSample) =>
    samples.map((s) => `${x(s.bytes)},${y(s[key] as number)}`).join(" ");

  return (
    <svg className="bench-chart" viewBox={`0 0 ${width} ${height}`} role="img" aria-label="parse time vs input size">
      {/* axes */}
      <line x1={pad.left} y1={pad.top} x2={pad.left} y2={pad.top + plotH} className="bench-axis" />
      <line x1={pad.left} y1={pad.top + plotH} x2={pad.left + plotW} y2={pad.top + plotH} className="bench-axis" />
      {/* reference straight line origin -> largest point (perfect linear) */}
      {samples.length ? (
        <line
          x1={x(0)}
          y1={y(0)}
          x2={x(samples[samples.length - 1].bytes)}
          y2={y(samples[samples.length - 1].parseMs)}
          className="bench-ideal"
        />
      ) : null}
      {/* wall-clock series (round-trip) */}
      <polyline className="bench-line bench-line-wall" points={line("wallMs")} fill="none" />
      {/* parse series (the payoff) */}
      <polyline className="bench-line bench-line-parse" points={line("parseMs")} fill="none" />
      {samples.map((s) => (
        <circle key={s.name} cx={x(s.bytes)} cy={y(s.parseMs)} r={3.5} className="bench-dot" />
      ))}
      {/* axis labels */}
      <text x={pad.left} y={height - 8} className="bench-axis-label">0</text>
      <text x={pad.left + plotW} y={height - 8} className="bench-axis-label" textAnchor="end">
        {fmtBytes(maxBytes)}
      </text>
      <text x={8} y={pad.top + 8} className="bench-axis-label">{fmtMs(maxMs)}</text>
      <text x={8} y={pad.top + plotH} className="bench-axis-label">0</text>
    </svg>
  );
}

export function BenchPanel({
  report,
  running,
  progress,
  onRun,
}: {
  report: BenchReport | null;
  running: boolean;
  progress: string;
  onRun: () => void;
}) {
  const linear = report ? report.scalingIndex <= 1.35 : false;
  return (
    <details className="panel bench-panel" open>
      <summary>
        <span className="panel-title">Benchmark</span>
        <span className="panel-meta">
          {report
            ? `${report.samples.length} samples · scaling ×${report.scalingIndex.toFixed(2)}/step ${linear ? "(linear)" : "(super-linear!)"}`
            : "size ladder"}
        </span>
      </summary>
      <div className="panel-body">
        <div className="bench-controls">
          <button type="button" className="btn btn-accent" onClick={onRun} disabled={running}>
            {running ? `Running… ${progress}` : "Run benchmark"}
          </button>
          <span className="bench-hint">
            Parses the current grammar's samples in size order; “parse” is the wasm run-parser time.
          </span>
        </div>
        {report ? (
          <>
            <ScalingChart samples={report.samples} />
            <div className="bench-legend">
              <span className="bench-legend-item bench-legend-parse">■ parse (wasm)</span>
              <span className="bench-legend-item bench-legend-wall">■ round-trip</span>
              <span className="bench-legend-item bench-legend-ideal">— ideal linear</span>
            </div>
            <table className="bench-table">
              <thead>
                <tr>
                  <th>sample</th>
                  <th>size</th>
                  <th>parse</th>
                  <th>round-trip</th>
                  <th>bytes/ms</th>
                  <th>×prev</th>
                </tr>
              </thead>
              <tbody>
                {report.samples.map((row) => (
                  <tr key={row.name}>
                    <td>{row.name}</td>
                    <td>{fmtBytes(row.bytes)}</td>
                    <td className="bench-num bench-num-parse">{fmtMs(row.parseMs)}</td>
                    <td className="bench-num">{fmtMs(row.wallMs)}</td>
                    <td className="bench-num">{row.bytesPerMs.toFixed(0)}</td>
                    <td className="bench-num">{row.xPrev ? `×${row.xPrev.toFixed(2)}` : "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {report.prepare.length ? (
              <p className="bench-prepare">
                prepare (once):{" "}
                {report.prepare.map((p) => `${p.name} ${fmtMs(p.ms)}`).join(" · ")}
              </p>
            ) : null}
          </>
        ) : null}
      </div>
    </details>
  );
}
