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

function colorForIndex(index) {
  const palette = [
    '#4cc9f0', '#f72585', '#b5179e', '#7209b7', '#560bad', '#480ca8',
    '#3a0ca3', '#3f37c9', '#4361ee', '#4895ef', '#06d6a0', '#ffd166',
    '#ef476f', '#118ab2', '#8338ec', '#ff006e', '#fb5607',
  ];
  return palette[index % palette.length];
}

function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function makeSeriesLabel(series, suffix = '') {
  return `${series.server_impl} ${series.transport}${suffix}`;
}

function seriesColor(series) {
  return series.server_impl === 'rust' ? '#f72585' : '#4cc9f0';
}

function seriesSymbol(series) {
  return series.transport === 'shm' ? 'diamond' : 'circle';
}

function seriesLineType(series) {
  return series.transport === 'shm' ? 'dashed' : 'solid';
}

const mountedCharts = [];
let resizeListenerInstalled = false;

function makeEchartsSeries(series, valueFn, { suffix = '', dash = false, colorOffset = 0 } = {}) {
  return series.map((s, index) => {
    const color = seriesColor(s) ?? colorForIndex(index + colorOffset);
    const data = s.points
      .map((p) => {
        const y = valueFn(p);
        return Number.isFinite(y) ? [p.offered_rps, y] : null;
      })
      .filter(Boolean);
    return {
      name: makeSeriesLabel(s, suffix),
      type: 'line',
      data,
      showSymbol: true,
      symbol: seriesSymbol(s),
      symbolSize: 6,
      connectNulls: false,
      lineStyle: {
        color,
        width: 2.5,
        type: dash ? 'dashed' : seriesLineType(s),
      },
      itemStyle: {
        color,
      },
      emphasis: {
        focus: 'series',
        blurScope: 'series',
        lineStyle: {
          width: 4,
        },
      },
      legendHoverLink: true,
    };
  });
}

function makeMemorySeries(series) {
  return series.map((s, index) => {
    const color = seriesColor(s) ?? colorForIndex(index);
    const data = s.points
      .map((p) => {
        if (!Number.isFinite(p.rss_mib)) {
          return null;
        }
        return {
          value: [p.offered_rps, p.rss_mib],
        };
      })
      .filter(Boolean);
    return {
      name: makeSeriesLabel(s),
      type: 'line',
      data,
      showSymbol: true,
      symbol: seriesSymbol(s),
      symbolSize: 6,
      connectNulls: false,
      lineStyle: {
        color,
        width: 2.5,
        type: seriesLineType(s),
      },
      itemStyle: {
        color,
      },
      emphasis: {
        focus: 'series',
        blurScope: 'series',
        lineStyle: {
          width: 4,
        },
      },
      legendHoverLink: true,
    };
  });
}

function attachHoverFocus(chart) {
  let activeName = null;

  const clear = () => {
    if (!activeName) return;
    chart.dispatchAction({ type: 'downplay', seriesIndex: 'all' });
    activeName = null;
  };

  chart.on('mouseover', (params) => {
    const name = params.componentType === 'legend' ? params.name : params.seriesName;
    if (!name || name === activeName) {
      return;
    }
    chart.dispatchAction({ type: 'downplay', seriesIndex: 'all' });
    chart.dispatchAction({ type: 'highlight', seriesName: name });
    activeName = name;
  });

  chart.on('mouseout', (params) => {
    if (params.componentType === 'legend' || params.componentType === 'series') {
      clear();
    }
  });

  chart.on('globalout', clear);
}

function mountChart(containerId, { series, yLabel, yFormat }) {
  const container = document.getElementById(containerId);
  if (!container) {
    return;
  }
  if (!window.echarts) {
    throw new Error('ECharts failed to load. Check the browser console and network access to the CDN.');
  }

  const chart = window.echarts.init(container, null, { renderer: 'canvas' });
  chart.setOption({
    backgroundColor: 'transparent',
    animationDuration: 200,
    color: series.map((_, index) => colorForIndex(index)),
    legend: {
      type: 'plain',
      show: true,
      top: 8,
      left: 16,
      right: 16,
      orient: 'horizontal',
      selectedMode: false,
      hoverLink: true,
      itemWidth: 30,
      itemHeight: 12,
      itemGap: 16,
      textStyle: { color: '#eef6fb', fontSize: 12 },
      inactiveColor: 'rgba(255,255,255,0.38)',
    },
    grid: {
      left: 70,
      right: 112,
      top: 88,
      bottom: 36,
      containLabel: true,
    },
    tooltip: {
      trigger: 'item',
      triggerOn: 'mousemove|click',
      backgroundColor: 'rgba(12, 18, 24, 0.96)',
      borderColor: 'rgba(255,255,255,0.12)',
      textStyle: { color: cssVar('--text') || '#eef6fb' },
      valueFormatter: (value) => yFormat(value),
      formatter: (params) => {
        const value = Array.isArray(params.value) ? params.value[1] : params.value;
        const lines = [
          `${params.seriesName}`,
          `offered rps: ${params.value?.[0] ?? 'n/a'}`,
          `${yLabel}: ${yFormat(value)}`,
        ];
        if (params.data && Object.prototype.hasOwnProperty.call(params.data, 'rss_mib')) {
          lines.push(`rss (MiB): ${formatNumber(params.data.rss_mib, 1)}`);
        }
        return lines.join('<br/>');
      },
    },
    xAxis: {
      type: 'value',
      name: 'offered rps',
      nameLocation: 'middle',
      nameGap: 28,
      max: (extent) => {
        const range = extent.max - extent.min;
        const pad = Math.max(8, range * 0.08);
        return extent.max + pad;
      },
      axisLine: { lineStyle: { color: 'rgba(255,255,255,0.35)' } },
      axisLabel: { color: cssVar('--muted') || '#99aebd' },
      splitLine: { lineStyle: { color: 'rgba(255,255,255,0.08)' } },
    },
    yAxis: {
      type: 'value',
      name: yLabel,
      nameLocation: 'middle',
      nameGap: 50,
      axisLine: { lineStyle: { color: 'rgba(255,255,255,0.35)' } },
      axisLabel: {
        color: cssVar('--muted') || '#99aebd',
        formatter: (value) => yFormat(value),
      },
      splitLine: { lineStyle: { color: 'rgba(255,255,255,0.08)' } },
    },
    series,
  });

  attachHoverFocus(chart);
  mountedCharts.push(chart);

  if (!resizeListenerInstalled) {
    resizeListenerInstalled = true;
    window.addEventListener('resize', () => {
      for (const mounted of mountedCharts) {
        mounted.resize();
      }
    });
  }
}

function renderLineChart(containerId, series, yLabel, yFormat) {
  mountChart(containerId, {
    series,
    yLabel,
    yFormat,
  });
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
    ? `Loaded ${tableRows.length} open-loop rows from ${series.length} series. Hover a line or legend entry to focus it. p99/p999 are shown once a series reaches ${minCompletedForP99} completed samples.`
    : `Loaded ${tableRows.length} open-loop rows from ${series.length} series and ${totalCompleted} completed samples. Hover a line or legend entry to focus it. p99/p999 are shown for any series with histogram data.`;
  renderSummary(tableRows);
  renderTable(tableRows);

  if (!window.echarts) {
    const status = document.getElementById('status');
    status.textContent = 'ECharts failed to load. Check network access to the CDN.';
    status.classList.add('error');
    return;
  }

  renderLineChart(
    'p99Plot',
    makeEchartsSeries(series, (p) => p.p99_us),
    'p99 latency (µs)',
    (value) => formatNumber(value, 0),
  );
  renderLineChart(
    'throughputPlot',
    makeEchartsSeries(series, (p) => p.achieved_rps),
    'achieved throughput (rps)',
    (value) => formatNumber(value, 0),
  );
  renderLineChart(
    'dropPlot',
    makeEchartsSeries(series, (p) => p.drop_rate_pct),
    'drop rate (%)',
    (value) => formatNumber(value, 1),
  );
  renderLineChart(
    'memoryPlot',
    makeMemorySeries(series),
    'memory (MiB)',
    (value) => formatNumber(value, 1),
  );
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
