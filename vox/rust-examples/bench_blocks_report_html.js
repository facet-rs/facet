#!/usr/bin/env node

import fs from 'node:fs';

function usage(code = 0) {
  const msg = 'usage: node rust-examples/bench_blocks_report_html.js --input /tmp/bench-blocks.json --output rust-examples/bench_blocks_report.html';
  (code === 0 ? console.log : console.error)(msg);
  process.exit(code);
}

function parseArgs(argv) {
  const out = {
    input: null,
    output: 'rust-examples/bench_blocks_report.html',
    title: 'Vox blocked benchmark report',
  };
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--input') {
      out.input = argv[++i];
    } else if (arg === '--output') {
      out.output = argv[++i];
    } else if (arg === '--title') {
      out.title = argv[++i];
    } else if (arg === '--help' || arg === '-h') {
      usage(0);
    } else {
      console.error(`unknown arg: ${arg}`);
      usage(1);
    }
  }
  if (!out.input) usage(1);
  return out;
}

function mean(xs) {
  return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : NaN;
}

function median(xs) {
  if (!xs.length) return NaN;
  const ys = [...xs].sort((a, b) => a - b);
  const mid = Math.floor(ys.length / 2);
  return ys.length % 2 === 0 ? (ys[mid - 1] + ys[mid]) / 2 : ys[mid];
}

function groupBy(rows, keyFn) {
  const out = new Map();
  for (const row of rows) {
    const key = keyFn(row);
    if (!out.has(key)) out.set(key, []);
    out.get(key).push(row);
  }
  return out;
}

function pctDelta(shm, local) {
  return ((shm / local) - 1) * 100;
}

function main() {
  const args = parseArgs(process.argv);
  const rows = JSON.parse(fs.readFileSync(args.input, 'utf8'));
  const byKey = groupBy(rows, (r) => `${r.payload_size}|${r.in_flight}`);
  const summaries = [];

  for (const [key, group] of [...byKey.entries()].sort((a, b) => {
    const [ap, ai] = a[0].split('|').map(Number);
    const [bp, bi] = b[0].split('|').map(Number);
    return ap - bp || ai - bi;
  })) {
    const [payloadSize, inFlight] = key.split('|').map(Number);
    const local = group.filter((r) => r.transport === 'local');
    const shm = group.filter((r) => r.transport === 'shm');
    const localByBlock = new Map(local.map((r) => [r.block, r]));
    const shmByBlock = new Map(shm.map((r) => [r.block, r]));
    const blocks = [...new Set([...localByBlock.keys(), ...shmByBlock.keys()])].sort((a, b) => a - b);

    const deltas = [];
    for (const block of blocks) {
      const l = localByBlock.get(block);
      const s = shmByBlock.get(block);
      if (!l || !s) continue;
      deltas.push({
        block,
        p50: pctDelta(s.p50_us, l.p50_us),
        p99: pctDelta(s.p99_us, l.p99_us),
        throughput: pctDelta(s.calls_per_sec, l.calls_per_sec),
        rss: l.peak_rss_kib && s.peak_rss_kib ? pctDelta(s.peak_rss_kib, l.peak_rss_kib) : null,
      });
    }

    summaries.push({
      payload_size: payloadSize,
      in_flight: inFlight,
      blocks: deltas.length,
      local_mean_us: mean(local.map((r) => r.per_call_micros)),
      shm_mean_us: mean(shm.map((r) => r.per_call_micros)),
      local_p50_us: median(local.map((r) => r.p50_us)),
      shm_p50_us: median(shm.map((r) => r.p50_us)),
      local_p99_us: median(local.map((r) => r.p99_us)),
      shm_p99_us: median(shm.map((r) => r.p99_us)),
      local_rps: mean(local.map((r) => r.calls_per_sec)),
      shm_rps: mean(shm.map((r) => r.calls_per_sec)),
      local_rss_kib: mean(local.map((r) => r.peak_rss_kib).filter((v) => Number.isFinite(v))),
      shm_rss_kib: mean(shm.map((r) => r.peak_rss_kib).filter((v) => Number.isFinite(v))),
      delta_p50_pct: mean(deltas.map((d) => d.p50)),
      delta_p99_pct: mean(deltas.map((d) => d.p99)),
      delta_throughput_pct: mean(deltas.map((d) => d.throughput)),
      delta_rss_pct: mean(deltas.map((d) => d.rss).filter((v) => Number.isFinite(v))),
    });
  }

  const html = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${args.title}</title>
  <script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script>
  <style>
    :root {
      --bg: #0a1116;
      --panel: #101920;
      --panel-2: #14212b;
      --text: #eef6fb;
      --muted: #99aebd;
      --local: #f7b955;
      --shm: #51d0c8;
      --good: #66e28b;
      --bad: #ff7d66;
      --grid: rgba(255,255,255,0.08);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      color: var(--text);
      font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background:
        radial-gradient(circle at top left, rgba(81,208,200,0.12), transparent 35%),
        radial-gradient(circle at top right, rgba(247,185,85,0.14), transparent 32%),
        linear-gradient(180deg, #081017, var(--bg));
    }
    .page { width: min(1280px, calc(100vw - 32px)); margin: 24px auto 40px; }
    .hero, .panel {
      background: linear-gradient(180deg, rgba(255,255,255,0.03), rgba(255,255,255,0.015));
      border: 1px solid rgba(255,255,255,0.08);
      border-radius: 20px;
      box-shadow: 0 18px 60px rgba(0,0,0,0.3);
    }
    .hero { padding: 26px 28px; margin-bottom: 18px; }
    h1 { margin: 0 0 8px; font-size: clamp(30px, 4vw, 52px); line-height: 0.95; }
    .sub { margin: 0; color: var(--muted); max-width: 80ch; line-height: 1.5; }
    .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 14px; margin-top: 20px; }
    .stat { background: rgba(255,255,255,0.025); border: 1px solid rgba(255,255,255,0.06); border-radius: 16px; padding: 14px 16px; }
    .stat .label { color: var(--muted); font-size: 12px; letter-spacing: .08em; text-transform: uppercase; margin-bottom: 8px; }
    .stat .value { font-size: 28px; font-weight: 700; }
    .layout { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; margin-bottom: 18px; }
    .layout.single { grid-template-columns: 1fr; }
    .panel { padding: 18px; }
    .panel h2 { margin: 0 0 6px; font-size: 22px; }
    .panel p { margin: 0 0 12px; color: var(--muted); }
    .plot { width: 100%; height: 420px; }
    .table-wrap { overflow-x: auto; border-radius: 16px; border: 1px solid rgba(255,255,255,0.08); }
    table { width: 100%; border-collapse: collapse; min-width: 980px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }
    thead th { text-align: left; color: var(--muted); padding: 12px 14px; border-bottom: 1px solid rgba(255,255,255,0.08); background: rgba(16,25,32,0.95); position: sticky; top: 0; }
    tbody td { padding: 10px 14px; border-top: 1px solid rgba(255,255,255,0.05); }
    tbody tr:hover td { background: rgba(255,255,255,0.03); }
    .good { color: var(--good); }
    .bad { color: var(--bad); }
    @media (max-width: 960px) { .layout { grid-template-columns: 1fr; } }
  </style>
</head>
<body>
  <div class="page">
    <section class="hero">
      <h1>${args.title}</h1>
      <p class="sub">Blocked fresh-process trials. Each row is summarized across blocks. Lower latency is better. Higher throughput is better. RSS is peak resident memory of the Swift subject during the trial.</p>
      <div class="stats" id="stats"></div>
    </section>
    <section class="layout">
      <div class="panel">
        <h2>Median latency</h2>
        <p>Median p50 latency across blocks, split by in-flight level.</p>
        <div class="plot" id="p50Plot"></div>
      </div>
      <div class="panel">
        <h2>Tail latency</h2>
        <p>Median p99 latency across blocks, split by in-flight level.</p>
        <div class="plot" id="p99Plot"></div>
      </div>
    </section>
    <section class="layout">
      <div class="panel">
        <h2>Throughput</h2>
        <p>Average achieved throughput across blocks.</p>
        <div class="plot" id="throughputPlot"></div>
      </div>
      <div class="panel">
        <h2>Peak RSS</h2>
        <p>Average peak resident set size for the Swift subject. This is process memory in RAM, not just heap.</p>
        <div class="plot" id="rssPlot"></div>
      </div>
    </section>
    <section class="layout single">
      <div class="panel">
        <h2>Transport deltas</h2>
        <p>Negative latency delta means SHM is faster. Positive throughput delta means SHM is faster. Positive RSS delta means SHM uses more memory.</p>
        <div class="plot" id="deltaHeatmap"></div>
      </div>
    </section>
    <section class="layout single">
      <div class="panel">
        <h2>Summary table</h2>
        <p>One row per condition.</p>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>payload</th>
                <th>in_flight</th>
                <th>blocks</th>
                <th>local p50 us</th>
                <th>shm p50 us</th>
                <th>local p99 us</th>
                <th>shm p99 us</th>
                <th>local rps</th>
                <th>shm rps</th>
                <th>local rss MiB</th>
                <th>shm rss MiB</th>
                <th>p50 delta %</th>
                <th>p99 delta %</th>
                <th>throughput delta %</th>
                <th>rss delta %</th>
              </tr>
            </thead>
            <tbody id="rows"></tbody>
          </table>
        </div>
      </div>
    </section>
  </div>
  <script>
    const summaries = ${JSON.stringify(summaries)};
    const inFlights = [...new Set(summaries.map((s) => s.in_flight))].sort((a, b) => a - b);
    const payloads = [...new Set(summaries.map((s) => s.payload_size))].sort((a, b) => a - b);
    const stats = document.getElementById('stats');
    const rowsEl = document.getElementById('rows');

    const totalBlocks = Math.max(...summaries.map((s) => s.blocks), 0);
    const avgP50Delta = summaries.reduce((a, s) => a + (s.delta_p50_pct || 0), 0) / summaries.length;
    const avgP99Delta = summaries.reduce((a, s) => a + (s.delta_p99_pct || 0), 0) / summaries.length;
    const avgThroughputDelta = summaries.reduce((a, s) => a + (s.delta_throughput_pct || 0), 0) / summaries.length;
    const avgRssDelta = summaries.reduce((a, s) => a + (s.delta_rss_pct || 0), 0) / summaries.length;

    for (const [label, value] of [
      ['conditions', String(summaries.length)],
      ['blocks per condition', String(totalBlocks)],
      ['avg p50 delta', avgP50Delta.toFixed(1) + '%'],
      ['avg p99 delta', avgP99Delta.toFixed(1) + '%'],
      ['avg throughput delta', avgThroughputDelta.toFixed(1) + '%'],
      ['avg rss delta', avgRssDelta.toFixed(1) + '%'],
    ]) {
      const el = document.createElement('div');
      el.className = 'stat';
      el.innerHTML = '<div class="label">' + label + '</div><div class="value">' + value + '</div>';
      stats.appendChild(el);
    }

    function linePlot(target, yKeyLocal, yKeyShm, titleSuffix, yAxisTitle) {
      const traces = [];
      for (const inFlight of inFlights) {
        const points = summaries.filter((s) => s.in_flight === inFlight);
        traces.push({
          x: points.map((s) => s.payload_size),
          y: points.map((s) => s[yKeyLocal]),
          mode: 'lines+markers',
          name: 'local i=' + inFlight,
          line: { color: '#f7b955' },
          marker: { symbol: 'circle', size: 8 },
        });
        traces.push({
          x: points.map((s) => s.payload_size),
          y: points.map((s) => s[yKeyShm]),
          mode: 'lines+markers',
          name: 'shm i=' + inFlight,
          line: { color: '#51d0c8', dash: 'dot' },
          marker: { symbol: 'diamond', size: 8 },
        });
      }
      Plotly.newPlot(target, traces, {
        paper_bgcolor: 'transparent',
        plot_bgcolor: 'transparent',
        font: { color: '#eef6fb' },
        margin: { l: 60, r: 20, t: 10, b: 50 },
        xaxis: {
          title: 'payload size',
          type: 'category',
          categoryorder: 'array',
          categoryarray: payloads,
          tickmode: 'array',
          tickvals: payloads,
          ticktext: payloads.map(String),
          gridcolor: 'rgba(255,255,255,0.08)'
        },
        yaxis: { title: yAxisTitle, gridcolor: 'rgba(255,255,255,0.08)' },
        legend: { orientation: 'h' },
      }, { displayModeBar: false, responsive: true });
    }

    linePlot('p50Plot', 'local_p50_us', 'shm_p50_us', 'p50', 'microseconds');
    linePlot('p99Plot', 'local_p99_us', 'shm_p99_us', 'p99', 'microseconds');
    linePlot('throughputPlot', 'local_rps', 'shm_rps', 'throughput', 'calls / sec');

    const rssTraces = [];
    for (const inFlight of inFlights) {
      const points = summaries.filter((s) => s.in_flight === inFlight);
      rssTraces.push({
        x: points.map((s) => s.payload_size),
        y: points.map((s) => s.local_rss_kib / 1024),
        mode: 'lines+markers',
        name: 'local i=' + inFlight,
        line: { color: '#f7b955' },
      });
      rssTraces.push({
        x: points.map((s) => s.payload_size),
        y: points.map((s) => s.shm_rss_kib / 1024),
        mode: 'lines+markers',
        name: 'shm i=' + inFlight,
        line: { color: '#51d0c8', dash: 'dot' },
      });
    }
    Plotly.newPlot('rssPlot', rssTraces, {
      paper_bgcolor: 'transparent',
      plot_bgcolor: 'transparent',
      font: { color: '#eef6fb' },
      margin: { l: 60, r: 20, t: 10, b: 50 },
      xaxis: {
        title: 'payload size',
        type: 'category',
        categoryorder: 'array',
        categoryarray: payloads,
        tickmode: 'array',
        tickvals: payloads,
        ticktext: payloads.map(String),
        gridcolor: 'rgba(255,255,255,0.08)'
      },
      yaxis: { title: 'MiB', gridcolor: 'rgba(255,255,255,0.08)' },
      legend: { orientation: 'h' },
    }, { displayModeBar: false, responsive: true });

    const heatZ = inFlights.map((inFlight) => payloads.map((payload) => {
      const s = summaries.find((row) => row.in_flight === inFlight && row.payload_size === payload);
      return s ? s.delta_p99_pct : null;
    }));
    Plotly.newPlot('deltaHeatmap', [{
      x: payloads,
      y: inFlights,
      z: heatZ,
      type: 'heatmap',
      colorscale: [
        [0.0, '#66e28b'],
        [0.5, '#202c33'],
        [1.0, '#ff7d66']
      ],
      zmid: 0,
      colorbar: { title: 'p99 delta %' },
    }], {
      paper_bgcolor: 'transparent',
      plot_bgcolor: 'transparent',
      font: { color: '#eef6fb' },
      margin: { l: 60, r: 20, t: 10, b: 50 },
      xaxis: { title: 'payload size', type: 'category' },
      yaxis: { title: 'in-flight', type: 'category' },
    }, { displayModeBar: false, responsive: true });

    for (const s of summaries) {
      const tr = document.createElement('tr');
      const cells = [
        s.payload_size,
        s.in_flight,
        s.blocks,
        s.local_p50_us.toFixed(1),
        s.shm_p50_us.toFixed(1),
        s.local_p99_us.toFixed(1),
        s.shm_p99_us.toFixed(1),
        s.local_rps.toFixed(0),
        s.shm_rps.toFixed(0),
        (s.local_rss_kib / 1024).toFixed(1),
        (s.shm_rss_kib / 1024).toFixed(1),
        s.delta_p50_pct.toFixed(1) + '%',
        s.delta_p99_pct.toFixed(1) + '%',
        s.delta_throughput_pct.toFixed(1) + '%',
        s.delta_rss_pct.toFixed(1) + '%',
      ];
      for (let i = 0; i < cells.length; i++) {
        const td = document.createElement('td');
        td.textContent = String(cells[i]);
        if (i >= 11 && i <= 12) td.className = Number(cells[i].replace('%', '')) <= 0 ? 'good' : 'bad';
        if (i === 13) td.className = Number(cells[i].replace('%', '')) >= 0 ? 'good' : 'bad';
        if (i === 14) td.className = Number(cells[i].replace('%', '')) <= 0 ? 'good' : 'bad';
        tr.appendChild(td);
      }
      rowsEl.appendChild(tr);
    }
  </script>
</body>
</html>`;

  fs.writeFileSync(args.output, html);
  console.log(`wrote ${args.output}`);
}

main();
