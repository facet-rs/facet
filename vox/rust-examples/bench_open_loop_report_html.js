#!/usr/bin/env node

import fs from 'node:fs';

function usage(code = 0) {
  const msg = 'usage: node rust-examples/bench_open_loop_report_html.js --input /tmp/open-loop-blocks.json --output /tmp/open-loop-report.html';
  (code === 0 ? console.log : console.error)(msg);
  process.exit(code);
}

function parseArgs(argv) {
  const out = {
    input: null,
    output: '/tmp/open-loop-report.html',
    title: 'Vox open-loop benchmark report',
    inFlights: null,
  };
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--input') out.input = argv[++i];
    else if (arg === '--output') out.output = argv[++i];
    else if (arg === '--title') out.title = argv[++i];
    else if (arg === '--in-flights') {
      out.inFlights = new Set(
        argv[++i]
          .split(',')
          .map((s) => Number.parseInt(s.trim(), 10))
          .filter((n) => Number.isFinite(n) && n > 0),
      );
    }
    else if (arg === '--help' || arg === '-h') usage(0);
    else {
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

function groupBy(rows, keyFn) {
  const out = new Map();
  for (const row of rows) {
    const key = keyFn(row);
    if (!out.has(key)) out.set(key, []);
    out.get(key).push(row);
  }
  return out;
}

function main() {
  const args = parseArgs(process.argv);
  const input = JSON.parse(fs.readFileSync(args.input, 'utf8'));
  const allRows = Array.isArray(input) ? input : input.rows;
  const rows = args.inFlights
    ? allRows.filter((r) => args.inFlights.has(Number(r.in_flight)))
    : allRows;
  const grouped = groupBy(rows, (r) => `${r.server_impl ?? 'swift'}|${r.transport}|${r.payload_size}|${r.in_flight}`);
  const series = [];
  const tableRows = [];

  for (const [key, group] of [...grouped.entries()].sort()) {
    const [serverImpl, transport, payloadSize, inFlight] = key.split('|');
    const byRate = groupBy(group, (r) => r.offered_rps);
    const points = [...byRate.entries()].sort((a, b) => Number(a[0]) - Number(b[0])).map(([offeredRps, trials]) => ({
      offered_rps: Number(offeredRps),
      server_impl: serverImpl,
      payload_size: Number(payloadSize),
      in_flight: Number(inFlight),
      transport,
      blocks: trials.length,
      baseline_rps: mean(trials.map((t) => t.baseline_rps)),
      achieved_rps: mean(trials.map((t) => t.calls_per_sec)),
      p50_us: mean(trials.map((t) => t.p50_us)),
      p99_us: mean(trials.map((t) => t.p99_us)),
      p999_us: mean(trials.map((t) => t.p999_us)),
      drop_rate_pct: mean(trials.map((t) => {
        const denom = (t.issued ?? 0) + (t.dropped ?? 0);
        return denom > 0 ? (t.dropped / denom) * 100 : 0;
      })),
      rss_mib: mean(trials.map((t) => (t.peak_rss_kib ?? 0) / 1024)),
      phys_footprint_mib: mean(trials.map((t) => (t.peak_phys_footprint_kib ?? 0) / 1024)),
    }));
    series.push({ server_impl: serverImpl, transport, payload_size: Number(payloadSize), in_flight: Number(inFlight), points });
    tableRows.push(...points);
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
      --text: #eef6fb;
      --muted: #99aebd;
      --local: #f7b955;
      --shm: #51d0c8;
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
    .page { width: min(1320px, calc(100vw - 32px)); margin: 24px auto 40px; }
    .hero, .panel {
      background: linear-gradient(180deg, rgba(255,255,255,0.03), rgba(255,255,255,0.015));
      border: 1px solid rgba(255,255,255,0.08);
      border-radius: 20px;
      box-shadow: 0 18px 60px rgba(0,0,0,0.3);
    }
    .hero { padding: 26px 28px; margin-bottom: 18px; }
    h1 { margin: 0 0 8px; font-size: clamp(30px, 4vw, 52px); line-height: 0.95; }
    .sub { margin: 0; color: var(--muted); max-width: 80ch; line-height: 1.5; }
    .layout { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; margin-bottom: 18px; }
    .layout.single { grid-template-columns: 1fr; }
    .panel { padding: 18px; }
    .panel h2 { margin: 0 0 6px; font-size: 22px; }
    .panel p { margin: 0 0 12px; color: var(--muted); }
    .plot { width: 100%; height: 460px; }
    .table-wrap { overflow-x: auto; border-radius: 16px; border: 1px solid rgba(255,255,255,0.08); }
    table { width: 100%; border-collapse: collapse; min-width: 1200px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }
    thead th { text-align: left; color: var(--muted); padding: 12px 14px; border-bottom: 1px solid rgba(255,255,255,0.08); background: rgba(16,25,32,0.95); position: sticky; top: 0; }
    tbody td { padding: 10px 14px; border-top: 1px solid rgba(255,255,255,0.05); }
    @media (max-width: 960px) { .layout { grid-template-columns: 1fr; } }
  </style>
</head>
<body>
  <div class="page">
    <section class="hero">
      <h1>${args.title}</h1>
      <p class="sub">Calibrated open-loop trials. X-axis is offered request rate, not completions. Each series is one payload/in-flight/transport combination averaged across blocks.</p>
    </section>
    <section class="layout">
      <div class="panel">
        <h2>p99 latency vs offered load</h2>
        <p>Tail latency should reveal the knee when queueing starts to dominate.</p>
        <div class="plot" id="p99Plot"></div>
      </div>
      <div class="panel">
        <h2>Achieved throughput vs offered load</h2>
        <p>If the line flattens while offered load rises, you are past saturation.</p>
        <div class="plot" id="throughputPlot"></div>
      </div>
    </section>
    <section class="layout">
      <div class="panel">
        <h2>Drop rate vs offered load</h2>
        <p>Drops here mean the scheduler wanted to inject more work but the configured in-flight budget was already full.</p>
        <div class="plot" id="dropPlot"></div>
      </div>
      <div class="panel">
        <h2>Peak Process Memory vs offered load</h2>
        <p>Solid = peak physical footprint (macOS). Dashed = peak RSS.</p>
        <div class="plot" id="memoryPlot"></div>
      </div>
    </section>
    <section class="layout single">
      <div class="panel">
        <h2>Summary table</h2>
        <p>One row per payload, concurrency, transport, and offered-load point.</p>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>transport</th>
                <th>server_impl</th>
                <th>payload</th>
                <th>in_flight</th>
                <th>blocks</th>
                <th>baseline_rps</th>
                <th>offered_rps</th>
                <th>achieved_rps</th>
                <th>p50_us</th>
                <th>p99_us</th>
                <th>p999_us</th>
                <th>drop_rate_pct</th>
                <th>rss_mib</th>
                <th>phys_footprint_mib</th>
              </tr>
            </thead>
            <tbody id="rows"></tbody>
          </table>
        </div>
      </div>
    </section>
  </div>
  <script>
    const series = ${JSON.stringify(series)};
    const rows = ${JSON.stringify(tableRows)};
    const palette = [
      '#4cc9f0', '#f72585', '#b5179e', '#7209b7', '#560bad', '#480ca8',
      '#3a0ca3', '#3f37c9', '#4361ee', '#4895ef', '#4cc9f0', '#06d6a0',
      '#ffd166', '#ef476f', '#118ab2', '#8338ec', '#ff006e', '#fb5607',
    ];
    const dashesByTransport = { local: 'solid', shm: 'dot' };
    const seriesKey = (s) => [s.server_impl, s.transport, s.payload_size, s.in_flight].join('|');
    const uniqueKeys = [...new Set(series.map(seriesKey))];
    const colorByKey = Object.fromEntries(
      uniqueKeys.map((k, i) => [k, palette[i % palette.length]]),
    );

    const singlePayload = new Set(series.map((s) => s.payload_size)).size === 1;
    const singleInflight = new Set(series.map((s) => s.in_flight)).size === 1;

    function shortLabel(s) {
      let label = s.server_impl + ' ' + s.transport;
      if (!singlePayload) label += ' payload=' + s.payload_size;
      if (!singleInflight) label += ' in_flight=' + s.in_flight;
      return label;
    }

    function tracesFor(metric) {
      return series.map((s) => ({
        x: s.points.map((p) => p.offered_rps),
        y: s.points.map((p) => p[metric]),
        mode: 'lines+markers',
        name: shortLabel(s),
        line: {
          color: colorByKey[seriesKey(s)] ?? '#ccc',
          dash: dashesByTransport[s.transport] ?? 'solid',
        },
        marker: { size: 7 },
      }));
    }

    function plot(target, metric, yTitle) {
      Plotly.newPlot(target, tracesFor(metric), {
        paper_bgcolor: 'transparent',
        plot_bgcolor: 'transparent',
        font: { color: '#eef6fb' },
        margin: { l: 70, r: 220, t: 10, b: 80 },
        xaxis: {
          title: { text: 'offered rps', standoff: 18 },
          automargin: true,
          gridcolor: 'rgba(255,255,255,0.08)',
        },
        yaxis: { title: yTitle, automargin: true, gridcolor: 'rgba(255,255,255,0.08)' },
        legend: { orientation: 'v', x: 1.02, xanchor: 'left', y: 1, yanchor: 'top' },
      }, { displayModeBar: false, responsive: true });
    }

    plot('p99Plot', 'p99_us', 'p99 latency (us)');
    plot('throughputPlot', 'achieved_rps', 'achieved throughput (rps)');
    plot('dropPlot', 'drop_rate_pct', 'drop rate (%)');
    Plotly.newPlot('memoryPlot', [
      ...series.map((s) => ({
        x: s.points.map((p) => p.offered_rps),
        y: s.points.map((p) => p.phys_footprint_mib),
        mode: 'lines+markers',
        name: shortLabel(s) + ' phys',
        line: {
          color: colorByKey[seriesKey(s)] ?? '#ccc',
          dash: dashesByTransport[s.transport] ?? 'solid',
        },
        marker: { size: 7 },
      })),
      ...series.map((s) => ({
        x: s.points.map((p) => p.offered_rps),
        y: s.points.map((p) => p.rss_mib),
        mode: 'lines+markers',
        name: shortLabel(s) + ' rss',
        line: { color: colorByKey[seriesKey(s)] ?? '#ccc', dash: 'dashdot' },
        marker: { size: 6, opacity: 0.6 },
      })),
    ], {
      paper_bgcolor: 'transparent',
      plot_bgcolor: 'transparent',
      font: { color: '#eef6fb' },
      margin: { l: 70, r: 220, t: 10, b: 80 },
      xaxis: {
        title: { text: 'offered rps', standoff: 18 },
        automargin: true,
        gridcolor: 'rgba(255,255,255,0.08)',
      },
      yaxis: { title: 'memory (MiB)', automargin: true, gridcolor: 'rgba(255,255,255,0.08)' },
      legend: { orientation: 'v', x: 1.02, xanchor: 'left', y: 1, yanchor: 'top' },
    }, { displayModeBar: false, responsive: true });

    const tbody = document.getElementById('rows');
    for (const row of rows.sort((a, b) => (a.server_impl ?? 'swift').localeCompare(b.server_impl ?? 'swift') || a.payload_size - b.payload_size || a.in_flight - b.in_flight || a.offered_rps - b.offered_rps || a.transport.localeCompare(b.transport))) {
      const tr = document.createElement('tr');
      for (const cell of [
        row.transport,
        row.server_impl ?? 'swift',
        row.payload_size,
        row.in_flight,
        row.blocks,
        row.baseline_rps.toFixed(0),
        row.offered_rps.toFixed(0),
        row.achieved_rps.toFixed(0),
        row.p50_us.toFixed(1),
        row.p99_us.toFixed(1),
        row.p999_us.toFixed(1),
        row.drop_rate_pct.toFixed(1),
        row.rss_mib.toFixed(1),
        row.phys_footprint_mib.toFixed(1),
      ]) {
        const td = document.createElement('td');
        td.textContent = String(cell);
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    }
  </script>
</body>
</html>`;

  fs.writeFileSync(args.output, html);
  console.log(`wrote ${args.output}`);
}

main();
