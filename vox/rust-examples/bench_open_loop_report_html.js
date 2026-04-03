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
  return {
    dataUrl: params.get('data') || DEFAULT_DATA_URL,
    title: params.get('title') || 'Vox open-loop benchmark report',
    minCompletedForP99: Number.parseInt(params.get('min-completed-for-p99') || '5000', 10),
  };
}

function formatNumber(value, digits = 1) {
  return Number.isFinite(value) ? value.toFixed(digits) : 'n/a';
}

function formatInt(value) {
  return Number.isFinite(value) ? Math.round(value).toString() : 'n/a';
}

function buildSeries(rows, minCompletedForP99) {
  const grouped = groupBy(rows, (r) => `${r.server_impl ?? 'swift'}|${r.transport}|${r.payload_size}|${r.in_flight}`);
  const series = [];
  const tableRows = [];

  for (const [key, group] of [...grouped.entries()].sort()) {
    const [serverImpl, transport, payloadSize, inFlight] = key.split('|');
    const byRate = groupBy(group, (r) => r.offered_rps);
    const points = [...byRate.entries()].sort((a, b) => Number(a[0]) - Number(b[0])).map(([offeredRps, trials]) => {
      const completedTotal = trials.reduce((acc, t) => acc + (t.completed ?? 0), 0);
      const pooled = pooledHistogram(trials);
      return {
        completed_total: completedTotal,
        offered_rps: Number(offeredRps),
        server_impl: serverImpl,
        payload_size: Number(payloadSize),
        in_flight: Number(inFlight),
        transport,
        blocks: trials.length,
        baseline_rps: mean(trials.map((t) => t.baseline_rps)),
        achieved_rps: mean(trials.map((t) => t.calls_per_sec)),
        p50_us: pooledQuantile(pooled, 0.50),
        p99_us: completedTotal >= minCompletedForP99 ? pooledQuantile(pooled, 0.99) : null,
        p999_us: completedTotal >= minCompletedForP99 ? pooledQuantile(pooled, 0.999) : null,
        drop_rate_pct: mean(trials.map((t) => {
          const denom = (t.issued ?? 0) + (t.dropped ?? 0);
          return denom > 0 ? (t.dropped / denom) * 100 : 0;
        })),
        rss_mib: meanFinite(trials.map((t) => Number.isFinite(t.peak_rss_kib) ? t.peak_rss_kib / 1024 : null)),
        phys_footprint_mib: meanFinite(trials.map((t) => Number.isFinite(t.peak_phys_footprint_kib) ? t.peak_phys_footprint_kib / 1024 : null)),
      };
    });
    series.push({
      server_impl: serverImpl,
      transport,
      payload_size: Number(payloadSize),
      in_flight: Number(inFlight),
      points,
    });
    tableRows.push(...points);
  }

  return { series, tableRows };
}

function lineStyle(series) {
  if (series.dash) {
    return { strokeDasharray: series.dash };
  }
  return series.transport === 'shm'
    ? { strokeDasharray: '4 4' }
    : { strokeDasharray: '' };
}

function colorForIndex(index) {
  const palette = [
    '#4cc9f0', '#f72585', '#b5179e', '#7209b7', '#560bad', '#480ca8',
    '#3a0ca3', '#3f37c9', '#4361ee', '#4895ef', '#06d6a0', '#ffd166',
    '#ef476f', '#118ab2', '#8338ec', '#ff006e', '#fb5607',
  ];
  return palette[index % palette.length];
}

function createSvgEl(tag) {
  return document.createElementNS('http://www.w3.org/2000/svg', tag);
}

function niceTicks(min, max, count = 5) {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
  if (min === max) return [min];
  const span = max - min;
  const step = niceStep(span / Math.max(1, count - 1));
  const start = Math.ceil(min / step) * step;
  const ticks = [];
  for (let tick = start; tick <= max + step * 0.5; tick += step) {
    ticks.push(tick);
  }
  return ticks;
}

function niceStep(rawStep) {
  const power = Math.pow(10, Math.floor(Math.log10(rawStep)));
  const normalized = rawStep / power;
  const nice =
    normalized <= 1 ? 1 :
    normalized <= 2 ? 2 :
    normalized <= 5 ? 5 : 10;
  return nice * power;
}

function renderChart(container, { title, yLabel, series, xFormat = formatInt, yFormat = formatNumber }) {
  const width = Math.max(container.clientWidth || 0, 320);
  const height = 360;
  const margin = { top: 28, right: 18, bottom: 48, left: 76 };
  const plotWidth = Math.max(1, width - margin.left - margin.right);
  const plotHeight = Math.max(1, height - margin.top - margin.bottom);

  const allPoints = series.flatMap((s) => s.points.map((p) => ({ x: p.offered_rps, y: p.y })));
  const xs = allPoints.map((p) => p.x).filter(Number.isFinite);
  const ys = allPoints.map((p) => p.y).filter(Number.isFinite);
  const xMin = xs.length ? Math.min(...xs) : 0;
  const xMax = xs.length ? Math.max(...xs) : 1;
  const yMin = ys.length ? Math.min(...ys) : 0;
  const yMax = ys.length ? Math.max(...ys) : 1;
  const xPad = xMin === xMax ? 1 : (xMax - xMin) * 0.05;
  const yPad = yMin === yMax ? 1 : (yMax - yMin) * 0.08;
  const x0 = xMin - xPad;
  const x1 = xMax + xPad;
  const y0 = yMin - yPad;
  const y1 = yMax + yPad;

  const xScale = (x) => margin.left + ((x - x0) / (x1 - x0)) * plotWidth;
  const yScale = (y) => margin.top + plotHeight - ((y - y0) / (y1 - y0)) * plotHeight;

  const svg = createSvgEl('svg');
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  svg.setAttribute('width', '100%');
  svg.setAttribute('height', String(height));
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', title);

  const background = createSvgEl('rect');
  background.setAttribute('x', '0');
  background.setAttribute('y', '0');
  background.setAttribute('width', String(width));
  background.setAttribute('height', String(height));
  background.setAttribute('rx', '16');
  background.setAttribute('fill', 'rgba(255,255,255,0.02)');
  svg.appendChild(background);

  const titleEl = createSvgEl('text');
  titleEl.setAttribute('x', String(margin.left));
  titleEl.setAttribute('y', '18');
  titleEl.setAttribute('fill', 'var(--text)');
  titleEl.setAttribute('font-size', '16');
  titleEl.setAttribute('font-weight', '700');
  titleEl.textContent = title;
  svg.appendChild(titleEl);

  const xTicks = niceTicks(x0, x1, 5);
  const yTicks = niceTicks(y0, y1, 5);

  for (const tick of xTicks) {
    const x = xScale(tick);
    const line = createSvgEl('line');
    line.setAttribute('x1', String(x));
    line.setAttribute('x2', String(x));
    line.setAttribute('y1', String(margin.top));
    line.setAttribute('y2', String(margin.top + plotHeight));
    line.setAttribute('stroke', 'rgba(255,255,255,0.08)');
    svg.appendChild(line);

    const label = createSvgEl('text');
    label.setAttribute('x', String(x));
    label.setAttribute('y', String(margin.top + plotHeight + 18));
    label.setAttribute('fill', 'var(--muted)');
    label.setAttribute('font-size', '11');
    label.setAttribute('text-anchor', 'middle');
    label.textContent = xFormat(tick);
    svg.appendChild(label);
  }

  for (const tick of yTicks) {
    const y = yScale(tick);
    const line = createSvgEl('line');
    line.setAttribute('x1', String(margin.left));
    line.setAttribute('x2', String(margin.left + plotWidth));
    line.setAttribute('y1', String(y));
    line.setAttribute('y2', String(y));
    line.setAttribute('stroke', 'rgba(255,255,255,0.08)');
    svg.appendChild(line);

    const label = createSvgEl('text');
    label.setAttribute('x', String(margin.left - 10));
    label.setAttribute('y', String(y + 4));
    label.setAttribute('fill', 'var(--muted)');
    label.setAttribute('font-size', '11');
    label.setAttribute('text-anchor', 'end');
    label.textContent = yFormat(tick);
    svg.appendChild(label);
  }

  const xAxis = createSvgEl('line');
  xAxis.setAttribute('x1', String(margin.left));
  xAxis.setAttribute('x2', String(margin.left + plotWidth));
  xAxis.setAttribute('y1', String(margin.top + plotHeight));
  xAxis.setAttribute('y2', String(margin.top + plotHeight));
  xAxis.setAttribute('stroke', 'rgba(255,255,255,0.35)');
  svg.appendChild(xAxis);

  const yAxis = createSvgEl('line');
  yAxis.setAttribute('x1', String(margin.left));
  yAxis.setAttribute('x2', String(margin.left));
  yAxis.setAttribute('y1', String(margin.top));
  yAxis.setAttribute('y2', String(margin.top + plotHeight));
  yAxis.setAttribute('stroke', 'rgba(255,255,255,0.35)');
  svg.appendChild(yAxis);

  for (const [index, s] of series.entries()) {
    const color = colorForIndex(index);
    const style = lineStyle(s);
    const validPoints = s.points.filter((p) => Number.isFinite(p.y));
    if (!validPoints.length) continue;

    const path = createSvgEl('path');
    path.setAttribute(
      'd',
      validPoints.map((p, i) => `${i === 0 ? 'M' : 'L'} ${xScale(p.offered_rps)} ${yScale(p.y)}`).join(' '),
    );
    path.setAttribute('fill', 'none');
    path.setAttribute('stroke', color);
    path.setAttribute('stroke-width', '2.5');
    if (style.strokeDasharray) path.setAttribute('stroke-dasharray', style.strokeDasharray);
    svg.appendChild(path);

    for (const point of validPoints) {
      const circle = createSvgEl('circle');
      circle.setAttribute('cx', String(xScale(point.offered_rps)));
      circle.setAttribute('cy', String(yScale(point.y)));
      circle.setAttribute('r', '3.5');
      circle.setAttribute('fill', color);
      svg.appendChild(circle);
    }
  }

  const xLabel = createSvgEl('text');
  xLabel.setAttribute('x', String(margin.left + plotWidth / 2));
  xLabel.setAttribute('y', String(height - 10));
  xLabel.setAttribute('fill', 'var(--muted)');
  xLabel.setAttribute('font-size', '12');
  xLabel.setAttribute('text-anchor', 'middle');
  xLabel.textContent = 'offered rps';
  svg.appendChild(xLabel);

  const yAxisLabel = createSvgEl('text');
  yAxisLabel.setAttribute('x', '16');
  yAxisLabel.setAttribute('y', String(margin.top + plotHeight / 2));
  yAxisLabel.setAttribute('fill', 'var(--muted)');
  yAxisLabel.setAttribute('font-size', '12');
  yAxisLabel.setAttribute('text-anchor', 'middle');
  yAxisLabel.setAttribute('transform', `rotate(-90 16 ${margin.top + plotHeight / 2})`);
  yAxisLabel.textContent = yLabel;
  svg.appendChild(yAxisLabel);

  const legend = document.createElement('div');
  legend.className = 'legend';
  for (const [index, s] of series.entries()) {
    const color = colorForIndex(index);
    const entry = document.createElement('div');
    entry.className = 'legend-entry';
    const swatch = document.createElement('span');
    swatch.className = 'swatch';
    swatch.style.background = color;
    const style = lineStyle(s);
    if (style.strokeDasharray) swatch.style.borderBottom = '2px dashed rgba(255,255,255,0.4)';
    const label = document.createElement('span');
    label.textContent = `${s.server_impl} ${s.transport}${s.payload_size ? ` payload=${s.payload_size}` : ''}${s.in_flight ? ` in_flight=${s.in_flight}` : ''}`;
    entry.append(swatch, label);
    legend.appendChild(entry);
  }

  container.innerHTML = '';
  container.append(svg, legend);
}

function renderTable(rows) {
  const tbody = document.getElementById('rows');
  tbody.textContent = '';
  const sorted = rows.slice().sort((a, b) => {
    const sa = a.server_impl ?? 'swift';
    const sb = b.server_impl ?? 'swift';
    return sa.localeCompare(sb)
      || a.payload_size - b.payload_size
      || a.in_flight - b.in_flight
      || a.offered_rps - b.offered_rps
      || a.transport.localeCompare(b.transport);
  });

  for (const row of sorted) {
    const tr = document.createElement('tr');
    const cells = [
      row.transport,
      row.server_impl ?? 'swift',
      row.payload_size,
      row.in_flight,
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
      formatNumber(row.phys_footprint_mib, 1),
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
  document.title = title;
  document.getElementById('title').textContent = title;
  document.getElementById('subtitle').textContent = `Loaded ${tableRows.length} open-loop rows from ${series.length} series. Refresh the JSON and reload the page to iterate on plots.`;
  renderSummary(tableRows);
  renderTable(tableRows);

  const groupedByChart = [
    {
      id: 'p99Plot',
      title: 'p99 latency vs offered load',
      yLabel: 'p99 latency (us)',
      series: series.map((s) => ({
        ...s,
        points: s.points.map((p) => ({ offered_rps: p.offered_rps, y: p.p99_us })),
      })),
      yFormat: (v) => formatNumber(v, 0),
    },
    {
      id: 'throughputPlot',
      title: 'achieved throughput vs offered load',
      yLabel: 'achieved throughput (rps)',
      series: series.map((s) => ({
        ...s,
        points: s.points.map((p) => ({ offered_rps: p.offered_rps, y: p.achieved_rps })),
      })),
      yFormat: (v) => formatNumber(v, 0),
    },
    {
      id: 'dropPlot',
      title: 'drop rate vs offered load',
      yLabel: 'drop rate (%)',
      series: series.map((s) => ({
        ...s,
        points: s.points.map((p) => ({ offered_rps: p.offered_rps, y: p.drop_rate_pct })),
      })),
      yFormat: (v) => formatNumber(v, 1),
    },
    {
      id: 'memoryPlot',
      title: 'peak process memory vs offered load',
      yLabel: 'memory (MiB)',
      series: [
        ...series.map((s) => ({
          ...s,
          dash: '4 4',
          points: s.points.map((p) => ({ offered_rps: p.offered_rps, y: p.phys_footprint_mib })),
          metric: 'phys',
        })),
        ...series.map((s) => ({
          ...s,
          transport: `${s.transport} rss`,
          dash: '10 4 2 4',
          points: s.points.map((p) => ({ offered_rps: p.offered_rps, y: p.rss_mib })),
          metric: 'rss',
        })),
      ],
      yFormat: (v) => formatNumber(v, 1),
    },
  ];

  for (const chart of groupedByChart) {
    const container = document.getElementById(chart.id);
    renderChart(container, {
      title: chart.title,
      yLabel: chart.yLabel,
      series: chart.series,
      yFormat: chart.yFormat,
    });
  }
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
