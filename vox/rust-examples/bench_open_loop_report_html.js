const DEFAULT_DATA_URL = './data.json';

function mean(xs) {
  return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : NaN;
}

function meanFinite(xs) {
  const finite = xs.filter((x) => Number.isFinite(x));
  return finite.length ? mean(finite) : null;
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

function pooledHistogram(trials) {
  const counts = new Map();
  let total = 0;
  for (const trial of trials) {
    for (const bin of trial.histogram ?? []) {
      const value = Number(bin.value_us);
      const count = Number(bin.count);
      if (!Number.isFinite(value) || !Number.isFinite(count) || count <= 0) continue;
      counts.set(value, (counts.get(value) ?? 0) + count);
      total += count;
    }
  }
  return {
    entries: [...counts.entries()].sort((a, b) => a[0] - b[0]),
    total,
  };
}

function pooledQuantile(pooled, q) {
  if (!pooled.total) return null;
  if (q <= 0) return pooled.entries[0]?.[0] ?? null;
  if (q >= 1) return pooled.entries.at(-1)?.[0] ?? null;
  const target = Math.max(1, Math.ceil(q * pooled.total));
  let seen = 0;
  for (const [value, count] of pooled.entries) {
    seen += count;
    if (seen >= target) return value;
  }
  return pooled.entries.at(-1)?.[0] ?? null;
}

function parseArgs() {
  const params = new URLSearchParams(window.location.search);
  const parsedMinCompletedForP99 = Number.parseInt(
    params.get('min-completed-for-p99') || '0',
    10,
  );
  return {
    dataUrl: params.get('data') || DEFAULT_DATA_URL,
    title: params.get('title') || 'Vox open-loop benchmark report',
    minCompletedForP99: Number.isFinite(parsedMinCompletedForP99)
      ? Math.max(0, parsedMinCompletedForP99)
      : 0,
  };
}

function formatNumber(value, digits = 1) {
  return Number.isFinite(value) ? value.toFixed(digits) : 'n/a';
}

function formatInt(value) {
  return Number.isFinite(value) ? Math.round(value).toString() : 'n/a';
}

function buildSeries(rows, minCompletedForP99) {
  const grouped = groupBy(rows, (r) => `${r.server_impl ?? 'swift'}|${r.transport}`);
  const series = [];
  const tableRows = [];
  const runtimeRank = { swift: 0, rust: 1 };
  const transportRank = { local: 0, shm: 1 };

  for (const [key, group] of [...grouped.entries()].sort(([a], [b]) => {
    const [aRuntime, aTransport] = a.split('|');
    const [bRuntime, bTransport] = b.split('|');
    return (runtimeRank[aRuntime] ?? 99) - (runtimeRank[bRuntime] ?? 99)
      || (transportRank[aTransport] ?? 99) - (transportRank[bTransport] ?? 99)
      || a.localeCompare(b);
  })) {
    const [serverImpl, transport] = key.split('|');
    const byRate = groupBy(group, (r) => r.offered_rps);
    const points = [...byRate.entries()].sort((a, b) => Number(a[0]) - Number(b[0])).map(([offeredRps, trials]) => {
      const completedTotal = trials.reduce((acc, t) => acc + (t.completed ?? 0), 0);
      const pooled = pooledHistogram(trials);
      const hasCompletedSamples = completedTotal > 0 && pooled.total > 0;
      return {
        completed_total: completedTotal,
        offered_rps: Number(offeredRps),
        server_impl: serverImpl,
        transport,
        blocks: trials.length,
        baseline_rps: mean(trials.map((t) => t.baseline_rps)),
        achieved_rps: mean(trials.map((t) => t.calls_per_sec)),
        p50_us: pooledQuantile(pooled, 0.50),
        p99_us: hasCompletedSamples && completedTotal >= minCompletedForP99 ? pooledQuantile(pooled, 0.99) : null,
        p999_us: hasCompletedSamples && completedTotal >= minCompletedForP99 ? pooledQuantile(pooled, 0.999) : null,
        p99_block_min: (() => {
          const vals = trials.map((t) => {
            const h = pooledHistogram([t]);
            return h.total > 0 ? pooledQuantile(h, 0.99) : null;
          }).filter((v) => v !== null);
          return vals.length ? Math.min(...vals) : null;
        })(),
        p99_block_max: (() => {
          const vals = trials.map((t) => {
            const h = pooledHistogram([t]);
            return h.total > 0 ? pooledQuantile(h, 0.99) : null;
          }).filter((v) => v !== null);
          return vals.length ? Math.max(...vals) : null;
        })(),
        drop_rate_pct: mean(trials.map((t) => {
          const denom = (t.issued ?? 0) + (t.dropped ?? 0);
          return denom > 0 ? (t.dropped / denom) * 100 : 0;
        })),
        rss_mib: meanFinite(trials.map((t) => Number.isFinite(t.peak_rss_kib) ? t.peak_rss_kib / 1024 : null)),
      };
    });
    series.push({
      server_impl: serverImpl,
      transport,
      points,
    });
    tableRows.push(...points);
  }

  return { series, tableRows };
}

function seriesColor(s) {
  if (s.server_impl === 'rust') {
    return s.transport === 'shm' ? '#ff9f1c' : '#f72585';
  } else {
    return s.transport === 'shm' ? '#7209b7' : '#4cc9f0';
  }
}

function makeSeriesLabel(s) {
  return `${s.server_impl} ${s.transport}`;
}

function hexToRgba(hex, alpha) {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

function makePlotlyTraces(series, valueFn, { minFn, maxFn } = {}) {
  const traces = [];
  for (const s of series) {
    const color = seriesColor(s);
    const name = makeSeriesLabel(s);
    const dash = s.transport === 'shm' ? 'dash' : 'solid';
    const symbol = s.transport === 'shm' ? 'diamond' : 'circle';

    const x = [], y = [], yMin = [], yMax = [];
    for (const p of s.points) {
      const val = valueFn(p);
      if (!Number.isFinite(val)) continue;
      x.push(p.offered_rps);
      y.push(val);
      if (minFn && maxFn) {
        yMin.push(minFn(p) ?? val);
        yMax.push(maxFn(p) ?? val);
      }
    }

    if (minFn && maxFn && x.length) {
      traces.push({
        x: [...x, ...x.slice().reverse()],
        y: [...yMax, ...yMin.slice().reverse()],
        fill: 'toself',
        fillcolor: hexToRgba(color, 0.15),
        line: { width: 0 },
        mode: 'lines',
        showlegend: false,
        hoverinfo: 'skip',
        name,
      });
    }

    traces.push({
      x, y,
      name,
      mode: 'lines+markers',
      line: { color, dash, width: 2.5 },
      marker: { symbol, size: 7, color },
    });
  }
  return traces;
}

const DARK_LAYOUT = {
  paper_bgcolor: 'transparent',
  plot_bgcolor: 'transparent',
  font: {
    color: '#eef6fb',
    family: 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    size: 12,
  },
  legend: {
    bgcolor: 'rgba(0,0,0,0)',
    font: { color: '#eef6fb', size: 12 },
    orientation: 'h',
    x: 0,
    y: 1.18,
    xanchor: 'left',
    yanchor: 'top',
  },
  margin: { l: 60, r: 20, t: 60, b: 50 },
  hovermode: 'closest',
};

function axisStyle(title) {
  return {
    title: { text: title, font: { color: '#99aebd', size: 12 } },
    gridcolor: 'rgba(255,255,255,0.08)',
    linecolor: 'rgba(255,255,255,0.35)',
    tickfont: { color: '#99aebd' },
    zerolinecolor: 'rgba(255,255,255,0.15)',
    rangemode: 'tozero',
  };
}

function mountChart(containerId, traces, xLabel, yLabel) {
  const container = document.getElementById(containerId);
  if (!container) return;

  const layout = {
    ...DARK_LAYOUT,
    xaxis: axisStyle(xLabel),
    yaxis: axisStyle(yLabel),
  };

  Plotly.newPlot(container, traces, layout, { responsive: true, displayModeBar: false });
}

function renderTable(rows) {
  const tbody = document.getElementById('rows');
  tbody.textContent = '';
  const sorted = rows.slice().sort((a, b) => {
    const rank = { swift: 0, rust: 1 };
    const ta = a.transport ?? '';
    const tb = b.transport ?? '';
    const sa = a.server_impl ?? 'swift';
    const sb = b.server_impl ?? 'swift';
    return (rank[sa] ?? 99) - (rank[sb] ?? 99)
      || ta.localeCompare(tb)
      || a.offered_rps - b.offered_rps
      || sa.localeCompare(sb);
  });

  for (const row of sorted) {
    const tr = document.createElement('tr');
    const cells = [
      row.transport,
      row.server_impl ?? 'swift',
      row.blocks,
      formatInt(row.completed_total),
      formatNumber(row.baseline_rps, 0),
      formatNumber(row.offered_rps, 0),
      formatNumber(row.achieved_rps, 0),
      formatNumber(row.p50_us, 1),
      formatNumber(row.p99_us, 1),
      formatNumber(row.p999_us, 1),
      formatNumber(row.drop_rate_pct, 1),
      formatNumber(row.rss_mib, 1),
    ];
    for (const cell of cells) {
      const td = document.createElement('td');
      td.textContent = String(cell);
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
}

function renderSummary(rows) {
  const el = document.getElementById('summary');
  const totalCompleted = rows.reduce((acc, row) => acc + (row.completed_total ?? 0), 0);
  const avgDrop = mean(rows.map((r) => r.drop_rate_pct));
  const avgThroughput = mean(rows.map((r) => r.achieved_rps));
  el.innerHTML = `
    <div class="stat"><div class="label">rows</div><div class="value">${rows.length}</div></div>
    <div class="stat"><div class="label">completed</div><div class="value">${formatInt(totalCompleted)}</div></div>
    <div class="stat"><div class="label">avg throughput</div><div class="value">${formatNumber(avgThroughput, 0)}</div></div>
    <div class="stat"><div class="label">avg drop rate</div><div class="value">${formatNumber(avgDrop, 1)}%</div></div>
  `;
}

function renderApp(data, title) {
  const { minCompletedForP99 } = parseArgs();
  const { series, tableRows } = buildSeries(data.rows ?? [], minCompletedForP99);
  const totalCompleted = tableRows.reduce((acc, row) => acc + (row.completed_total ?? 0), 0);
  document.title = title;
  document.getElementById('title').textContent = title;
  document.getElementById('subtitle').textContent = minCompletedForP99 > 0
    ? `Loaded ${tableRows.length} open-loop rows from ${series.length} series. p99/p999 are shown once a series reaches ${minCompletedForP99} completed samples.`
    : `Loaded ${tableRows.length} open-loop rows from ${series.length} series and ${totalCompleted} completed samples. p99/p999 are shown for any series with histogram data.`;
  renderSummary(tableRows);
  renderTable(tableRows);

  if (!window.Plotly) {
    const status = document.getElementById('status');
    status.textContent = 'Plotly failed to load. Check network access to the CDN.';
    status.classList.add('error');
    return;
  }

  mountChart('p99Plot', makePlotlyTraces(series, (p) => p.p99_us, { minFn: (p) => p.p99_block_min, maxFn: (p) => p.p99_block_max }), 'offered rps', 'p99 latency (µs)');
  mountChart('throughputPlot', makePlotlyTraces(series, (p) => p.achieved_rps), 'offered rps', 'achieved throughput (rps)');
  mountChart('dropPlot', makePlotlyTraces(series, (p) => p.drop_rate_pct), 'offered rps', 'drop rate (%)');
  mountChart('memoryPlot', makePlotlyTraces(series, (p) => p.rss_mib), 'offered rps', 'memory (MiB)');
}

async function main() {
  const { dataUrl, title } = parseArgs();
  const status = document.getElementById('status');
  status.textContent = `Loading ${dataUrl}...`;
  try {
    const response = await fetch(dataUrl, { cache: 'no-store' });
    if (!response.ok) {
      throw new Error(`failed to fetch ${dataUrl}: ${response.status} ${response.statusText}`);
    }
    const data = await response.json();
    renderApp(data, title);
    status.textContent = `Loaded ${data.rows?.length ?? 0} rows from ${dataUrl}`;
  } catch (err) {
    status.textContent = `Failed to load ${dataUrl}: ${err instanceof Error ? err.message : String(err)}`;
    status.classList.add('error');
    throw err;
  }
}

window.addEventListener('DOMContentLoaded', () => {
  void main();
});
