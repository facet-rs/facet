// Unified SPA for perf.facet.rs
// Hash-based routing: /#/ (index), /#/runs/:branch/:commit/:op (report)

import { h, render } from 'https://esm.sh/preact@10.19.3';
import { useState, useEffect, useCallback, useMemo, useRef } from 'https://esm.sh/preact@10.19.3/hooks';
import htm from 'https://esm.sh/htm@3.1.1';

const html = htm.bind(h);

// ============================================================================
// Hash Router
// ============================================================================

function parseHash() {
  const hash = location.hash.slice(1) || '/';
  const [path, query] = hash.split('?');
  const segments = path.split('/').filter(Boolean);
  return { path, segments, query };
}

function useHashRouter() {
  const [route, setRoute] = useState(parseHash);

  useEffect(() => {
    const handler = () => setRoute(parseHash());
    window.addEventListener('hashchange', handler);
    return () => window.removeEventListener('hashchange', handler);
  }, []);

  const navigate = useCallback((path) => {
    location.hash = path;
  }, []);

  return { ...route, navigate };
}

function matchRoute(segments, pattern) {
  const patternParts = pattern.split('/').filter(Boolean);
  if (segments.length < patternParts.length) return null;

  const params = {};
  for (let i = 0; i < patternParts.length; i++) {
    const part = patternParts[i];
    if (part.startsWith(':')) {
      params[part.slice(1)] = segments[i];
    } else if (part !== segments[i]) {
      return null;
    }
  }
  return params;
}

// ============================================================================
// Data Layer
// ============================================================================

const runCache = new Map();
let indexDataCache = null;

async function fetchIndexData() {
  if (indexDataCache) return indexDataCache;

  try {
    let response = await fetch('/index-v2.json');
    if (!response.ok) response = await fetch('/index.json');
    if (!response.ok) throw new Error('Failed to load index');
    indexDataCache = await response.json();
    return indexDataCache;
  } catch (e) {
    console.error('Failed to fetch index:', e);
    return null;
  }
}

async function fetchRunData(url) {
  if (runCache.has(url)) return runCache.get(url);

  try {
    const response = await fetch(url);
    if (!response.ok) return null;
    const data = await response.json();
    runCache.set(url, data);
    return data;
  } catch (e) {
    console.error(`Failed to fetch ${url}:`, e);
    return null;
  }
}

// ============================================================================
// Utility Functions
// ============================================================================

function formatNumber(n) {
  if (n === null || n === undefined) return '—';
  return n.toLocaleString();
}

function formatDelta(delta) {
  const EPSILON = 0.5;
  if (Math.abs(delta) < EPSILON) {
    return { text: `${delta > 0 ? '+' : ''}${delta.toFixed(1)}%`, color: 'var(--neutral)', icon: '▬' };
  }
  const sign = delta > 0 ? '+' : '';
  return {
    text: `${sign}${delta.toFixed(1)}%`,
    color: delta < 0 ? 'var(--good)' : 'var(--bad)',
    icon: delta < 0 ? '▲' : '▼'
  };
}

function formatRelativeTime(input) {
  if (!input) return '—';
  const date = typeof input === 'number' ? new Date(input * 1000) : new Date(input);
  const diffMs = Date.now() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  if (diffMin < 1) return 'just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHour < 24) return `${diffHour}h ago`;
  if (diffDay < 30) return `${diffDay}d ago`;
  return `${Math.floor(diffDay / 30)}mo ago`;
}

function formatAbsoluteTime(input) {
  if (!input) return '';
  const date = typeof input === 'number' ? new Date(input * 1000) : new Date(input);
  return new Intl.DateTimeFormat(undefined, {
    year: 'numeric', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit'
  }).format(date);
}

function formatMetricValue(value, metricId) {
  if (value === null || value === undefined) return '—';
  if (metricId === 'time_median_ns') {
    if (value >= 1e9) return `${(value / 1e9).toFixed(2)}s`;
    if (value >= 1e6) return `${(value / 1e6).toFixed(2)}ms`;
    if (value >= 1e3) return `${(value / 1e3).toFixed(2)}μs`;
    return `${value.toFixed(1)}ns`;
  }
  return formatNumber(Math.round(value));
}

function computeRatio(runData, operation = 'deserialize', metric = 'instructions') {
  if (!runData?.results) return null;
  let facetTotal = 0, serdeTotal = 0;

  for (const caseData of Object.values(runData.results)) {
    const facetResult = caseData?.targets?.facet_format_jit?.ops?.[operation];
    if (facetResult?.ok) facetTotal += facetResult.metrics?.[metric] || 0;

    const serdeResult = caseData?.targets?.serde_json?.ops?.[operation];
    if (serdeResult?.ok) serdeTotal += serdeResult.metrics?.[metric] || 0;
  }

  return serdeTotal > 0 ? facetTotal / serdeTotal : null;
}

// ============================================================================
// Shared Components
// ============================================================================

function Link({ href, children, ...props }) {
  const onClick = useCallback((e) => {
    if (!e.ctrlKey && !e.metaKey && !e.shiftKey) {
      e.preventDefault();
      location.hash = href;
    }
  }, [href]);

  return html`<a href="#${href}" onClick=${onClick} ...${props}>${children}</a>`;
}

function Dropdown({ trigger, items, value, onChange }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e) => {
      if (ref.current && !ref.current.contains(e.target)) setOpen(false);
    };
    const escHandler = (e) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('click', handler);
    document.addEventListener('keydown', escHandler);
    return () => {
      document.removeEventListener('click', handler);
      document.removeEventListener('keydown', escHandler);
    };
  }, [open]);

  return html`
    <div class="dropdown" ref=${ref}>
      <button class="dropdown-trigger" onClick=${() => setOpen(!open)}>
        ${trigger} <span class="dropdown-arrow">▼</span>
      </button>
      ${open && html`
        <div class="dropdown-menu">
          ${items.map(item => html`
            <button
              key=${item.value}
              class="dropdown-item ${item.value === value ? 'active' : ''}"
              onClick=${() => { onChange(item.value); setOpen(false); }}
            >
              <span class="dropdown-label">${item.label}</span>
              ${item.meta && html`<span class="dropdown-meta">${item.meta}</span>`}
            </button>
          `)}
        </div>
      `}
    </div>
  `;
}

// ============================================================================
// Index Page Components
// ============================================================================

function IndexPage() {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [filter, setFilter] = useState('');
  const [expandedBranch, setExpandedBranch] = useState(null);
  const [baselineRatio, setBaselineRatio] = useState(null);

  useEffect(() => {
    fetchIndexData().then(d => {
      if (d) setData(d);
      else setError('Failed to load index data');
      setLoading(false);
    });
  }, []);

  // Load baseline ratio
  useEffect(() => {
    if (!data?.baseline?.run_json_url) return;
    fetchRunData(data.baseline.run_json_url).then(run => {
      if (run) setBaselineRatio(computeRatio(run));
    });
  }, [data?.baseline?.run_json_url]);

  if (loading) return html`<div class="loading">Loading...</div>`;
  if (error) return html`<div class="error">${error}</div>`;
  if (!data) return html`<div class="error">No data</div>`;

  const branches = Object.entries(data.branches || {})
    .filter(([key]) => !filter || key.toLowerCase().includes(filter.toLowerCase()))
    .sort(([a], [b]) => {
      if (a === 'main') return -1;
      if (b === 'main') return 1;
      return a.localeCompare(b);
    });

  return html`
    <div class="index-page">
      <header class="page-header">
        <h1>facet performance benchmarks</h1>
        <p class="subtitle">Comparing facet-format+jit vs serde_json</p>
        <input
          type="text"
          class="filter-input"
          placeholder="Filter branches..."
          value=${filter}
          onInput=${(e) => setFilter(e.target.value)}
        />
      </header>

      ${data.baseline && baselineRatio && html`
        <div class="baseline-banner">
          <span class="baseline-label">Baseline: main</span>
          <span class="baseline-value">${(baselineRatio * 100).toFixed(1)}% of serde</span>
          <${Link} href="/runs/main/${data.baseline.commit_sha}/deserialize" class="baseline-link">
            view report
          <//>
        </div>
      `}

      <div class="branch-list">
        ${branches.map(([key, info]) => html`
          <${BranchRow}
            key=${key}
            branchKey=${key}
            info=${info}
            commits=${data.branch_commits?.[key] || []}
            expanded=${expandedBranch === key}
            onToggle=${() => setExpandedBranch(expandedBranch === key ? null : key)}
            baselineRatio=${baselineRatio}
            allCommits=${data.commits}
          />
        `)}
      </div>
    </div>
  `;
}

function BranchRow({ branchKey, info, commits, expanded, onToggle, baselineRatio, allCommits }) {
  const [ratio, setRatio] = useState(null);
  const latest = commits[0];
  const commitData = latest ? allCommits?.[latest.sha] : null;
  const subject = commitData?.subject || '(no message)';

  useEffect(() => {
    if (!latest?.run_json_url) return;
    fetchRunData(latest.run_json_url).then(run => {
      if (run) setRatio(computeRatio(run));
    });
  }, [latest?.run_json_url]);

  const delta = ratio && baselineRatio ? ((ratio - baselineRatio) / baselineRatio) * 100 : null;
  const deltaInfo = delta !== null ? formatDelta(delta) : null;

  return html`
    <div class="branch-row ${expanded ? 'expanded' : ''}" onClick=${onToggle}>
      <div class="branch-main">
        <div class="branch-info">
          <div class="branch-name">${branchKey}</div>
          <div class="branch-subject">${subject}</div>
          <div class="branch-meta">
            ${commits.length} commit${commits.length !== 1 ? 's' : ''}
            ${latest && html` · ${formatRelativeTime(latest.timestamp_unix)}`}
          </div>
        </div>
        <div class="branch-result">
          ${ratio && html`<span class="result-value">${(ratio * 100).toFixed(1)}% of serde</span>`}
          ${deltaInfo && html`
            <span class="result-delta" style="color: ${deltaInfo.color}">
              ${deltaInfo.icon} ${deltaInfo.text}
            </span>
          `}
        </div>
      </div>

      ${expanded && html`
        <div class="branch-expanded" onClick=${(e) => e.stopPropagation()}>
          <div class="result-links">
            <${Link} href="/runs/${branchKey}/${latest?.sha}/deserialize">
              View full report (deserialize)
            <//>
            <span> | </span>
            <${Link} href="/runs/${branchKey}/${latest?.sha}/serialize">serialize<//>
          </div>

          ${commits.length > 1 && html`
            <details class="commit-history">
              <summary>Commit history (${commits.length})</summary>
              <div class="commit-list">
                ${commits.slice(0, 10).map(c => {
                  const cd = allCommits?.[c.sha];
                  return html`
                    <${Link} key=${c.sha} class="commit-item" href="/runs/${branchKey}/${c.sha}/deserialize">
                      <span class="commit-subject">${cd?.subject || c.short}</span>
                      <span class="commit-meta">${c.short} · ${formatRelativeTime(c.timestamp_unix)}</span>
                    <//>
                  `;
                })}
              </div>
            </details>
          `}
        </div>
      `}
    </div>
  `;
}

// ============================================================================
// Report Page Components
// ============================================================================

function ReportPage({ branch, commit, operation }) {
  const [runData, setRunData] = useState(null);
  const [indexData, setIndexData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [selectedMetric, setSelectedMetric] = useState('instructions');
  const [selectedCase, setSelectedCase] = useState(null);
  const { navigate } = useHashRouter();

  const op = operation || 'deserialize';
  const runUrl = `/runs/${branch}/${commit}/run.json`;

  useEffect(() => {
    setLoading(true);
    Promise.all([fetchRunData(runUrl), fetchIndexData()]).then(([run, index]) => {
      if (run) {
        setRunData(run);
        // Select first case by default
        const firstGroup = run.groups?.[0];
        if (firstGroup?.cases?.[0]) setSelectedCase(firstGroup.cases[0].case_id);
      } else {
        setError('Failed to load benchmark data');
      }
      setIndexData(index);
      setLoading(false);
    });
  }, [runUrl]);

  if (loading) return html`<div class="loading">Loading report...</div>`;
  if (error) return html`<div class="error">${error}</div>`;
  if (!runData) return html`<div class="error">No data</div>`;

  const metrics = runData.schema?.metrics || [];
  const targets = runData.schema?.targets || [];
  const groups = runData.groups || [];

  // Build branch/commit dropdown items
  const branchItems = indexData?.branches ?
    Object.keys(indexData.branches).map(b => ({ value: b, label: b })) : [];
  const commitItems = indexData?.branch_commits?.[branch]?.map(c => ({
    value: c.sha,
    label: c.short,
    meta: formatRelativeTime(c.timestamp_unix)
  })) || [];

  return html`
    <div class="report-page">
      <nav class="report-nav">
        <div class="nav-left">
          <${Link} href="/" class="nav-home">← Index<//>
          <${Dropdown}
            trigger=${branch}
            items=${branchItems}
            value=${branch}
            onChange=${(b) => {
              const firstCommit = indexData?.branch_commits?.[b]?.[0]?.sha;
              if (firstCommit) navigate(`/runs/${b}/${firstCommit}/${op}`);
            }}
          />
          <span class="nav-sep">/</span>
          <${Dropdown}
            trigger=${commit.slice(0, 8)}
            items=${commitItems}
            value=${commit}
            onChange=${(c) => navigate(`/runs/${branch}/${c}/${op}`)}
          />
        </div>
        <div class="nav-right">
          <div class="op-toggle">
            <button
              class=${op === 'deserialize' ? 'active' : ''}
              onClick=${() => navigate(`/runs/${branch}/${commit}/deserialize`)}
            >deser</button>
            <button
              class=${op === 'serialize' ? 'active' : ''}
              onClick=${() => navigate(`/runs/${branch}/${commit}/serialize`)}
            >ser</button>
          </div>
          <${Dropdown}
            trigger=${metrics.find(m => m.id === selectedMetric)?.label || selectedMetric}
            items=${metrics.map(m => ({ value: m.id, label: m.label }))}
            value=${selectedMetric}
            onChange=${setSelectedMetric}
          />
        </div>
      </nav>

      <div class="report-layout">
        <aside class="report-sidebar">
          ${groups.map(group => html`
            <div key=${group.group_id} class="sidebar-group">
              <div class="group-label">${group.label}</div>
              ${group.cases.map(c => html`
                <button
                  key=${c.case_id}
                  class="sidebar-case ${selectedCase === c.case_id ? 'active' : ''}"
                  onClick=${() => setSelectedCase(c.case_id)}
                >
                  ${c.label}
                </button>
              `)}
            </div>
          `)}
        </aside>

        <main class="report-main">
          ${selectedCase && html`
            <${CaseView}
              caseId=${selectedCase}
              caseData=${runData.results?.[selectedCase]}
              targets=${targets}
              metrics=${metrics}
              selectedMetric=${selectedMetric}
              operation=${op}
            />
          `}
        </main>
      </div>
    </div>
  `;
}

function CaseView({ caseId, caseData, targets, metrics, selectedMetric, operation }) {
  if (!caseData) return html`<div class="no-data">No data for ${caseId}</div>`;

  const metricInfo = metrics.find(m => m.id === selectedMetric);
  const baseline = caseData.targets?.serde_json?.ops?.[operation];
  const baselineValue = baseline?.ok ? baseline.metrics?.[selectedMetric] : null;

  return html`
    <div class="case-view">
      <h2 class="case-title">${caseId}</h2>

      <table class="results-table">
        <thead>
          <tr>
            <th>Target</th>
            <th>${metricInfo?.label || selectedMetric}</th>
            <th>vs serde_json</th>
          </tr>
        </thead>
        <tbody>
          ${targets.map(target => {
            const result = caseData.targets?.[target.id]?.ops?.[operation];
            if (!result) return null;

            const value = result.ok ? result.metrics?.[selectedMetric] : null;
            const ratio = value && baselineValue ? value / baselineValue : null;
            const delta = ratio ? (ratio - 1) * 100 : null;
            const deltaInfo = delta !== null ? formatDelta(delta) : null;

            return html`
              <tr key=${target.id} class=${target.kind === 'baseline' ? 'baseline-row' : ''}>
                <td class="target-cell">
                  <span class="target-label">${target.label}</span>
                  ${target.kind === 'baseline' && html`<span class="baseline-tag">baseline</span>`}
                </td>
                <td class="value-cell">
                  ${result.ok ? formatMetricValue(value, selectedMetric) : html`
                    <span class="error-value" title=${result.error?.message}>error</span>
                  `}
                </td>
                <td class="delta-cell">
                  ${deltaInfo ? html`
                    <span style="color: ${deltaInfo.color}">${(ratio * 100).toFixed(1)}%</span>
                  ` : '—'}
                </td>
              </tr>
            `;
          })}
        </tbody>
      </table>

      <${MetricsDetail}
        caseData=${caseData}
        targets=${targets}
        metrics=${metrics}
        operation=${operation}
      />
    </div>
  `;
}

function MetricsDetail({ caseData, targets, metrics, operation }) {
  return html`
    <details class="metrics-detail">
      <summary>All metrics</summary>
      <div class="metrics-grid">
        ${targets.filter(t => caseData.targets?.[t.id]?.ops?.[operation]?.ok).map(target => {
          const result = caseData.targets[target.id].ops[operation];
          return html`
            <div key=${target.id} class="metrics-card">
              <div class="metrics-card-header">${target.label}</div>
              <div class="metrics-card-body">
                ${metrics.map(m => {
                  const val = result.metrics?.[m.id];
                  return val !== undefined && val !== null ? html`
                    <div key=${m.id} class="metric-row">
                      <span class="metric-label">${m.label}</span>
                      <span class="metric-value">${formatMetricValue(val, m.id)}</span>
                    </div>
                  ` : null;
                })}
              </div>
            </div>
          `;
        })}
      </div>
    </details>
  `;
}

// ============================================================================
// App Router
// ============================================================================

function App() {
  const { segments } = useHashRouter();

  // Route matching
  if (segments.length === 0 || (segments.length === 1 && segments[0] === '')) {
    return html`<${IndexPage} />`;
  }

  const reportMatch = matchRoute(segments, 'runs/:branch/:commit');
  if (reportMatch) {
    const operation = segments[4] || 'deserialize';
    return html`<${ReportPage} branch=${reportMatch.branch} commit=${reportMatch.commit} operation=${operation} />`;
  }

  return html`
    <div class="not-found">
      <h1>404</h1>
      <p>Page not found</p>
      <${Link} href="/">← Back to index<//>
    </div>
  `;
}

// ============================================================================
// Styles
// ============================================================================

const styles = `
/* Shared */
.loading, .error, .not-found {
  max-width: 1200px;
  margin: 2rem auto;
  padding: 2rem;
  text-align: center;
  color: var(--muted);
}
.error { color: var(--bad); }
.not-found h1 { font-size: 4rem; margin-bottom: 0.5rem; }

/* Index Page */
.index-page { max-width: 1200px; margin: 0 auto; }

.page-header {
  padding: 2rem 1rem;
  border-bottom: 1px solid var(--border);
}
.page-header h1 { margin-bottom: 0.25rem; }
.subtitle { color: var(--muted); font-size: 14px; margin-bottom: 1rem; }
.filter-input {
  padding: 0.5rem 0.75rem;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text);
  font-family: var(--mono);
  font-size: 13px;
  width: 100%;
  max-width: 300px;
}
.filter-input:focus {
  outline: 2px solid var(--accent);
  border-color: var(--accent);
}

.baseline-banner {
  padding: 0.75rem 1rem;
  background: var(--panel2);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  gap: 1rem;
  font-size: 14px;
}
.baseline-label { font-weight: 600; color: var(--muted); text-transform: uppercase; font-size: 12px; }
.baseline-value { font-weight: 600; }
.baseline-link { margin-left: auto; color: var(--accent); text-decoration: none; }
.baseline-link:hover { text-decoration: underline; }

.branch-list { }
.branch-row {
  border-bottom: 1px solid var(--border);
  cursor: pointer;
  transition: background 0.1s;
}
.branch-row:hover { background: var(--panel2); }
.branch-row.expanded { background: var(--panel); border-left: 3px solid var(--accent); }

.branch-main {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1rem;
  gap: 2rem;
}
.branch-info { flex: 1; min-width: 0; }
.branch-name { font-weight: 600; font-size: 15px; margin-bottom: 0.25rem; }
.branch-subject { color: var(--muted); font-size: 13px; margin-bottom: 0.25rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.branch-meta { font-size: 11px; color: var(--muted); }

.branch-result { text-align: right; white-space: nowrap; }
.result-value { font-weight: 600; font-size: 14px; display: block; }
.result-delta { font-weight: 600; font-size: 16px; }

.branch-expanded {
  padding: 1rem 1rem 1rem 1.5rem;
  border-top: 1px solid var(--border);
}
.result-links { font-size: 13px; margin-bottom: 1rem; }
.result-links a { color: var(--accent); text-decoration: none; }
.result-links a:hover { text-decoration: underline; }

.commit-history { margin-top: 1rem; }
.commit-history summary { cursor: pointer; font-weight: 600; font-size: 13px; padding: 0.5rem 0; }
.commit-list { margin-top: 0.5rem; }
.commit-item {
  display: block;
  padding: 0.5rem;
  text-decoration: none;
  border-radius: 4px;
  margin-bottom: 2px;
}
.commit-item:hover { background: var(--panel2); }
.commit-subject { color: var(--text); font-size: 13px; display: block; margin-bottom: 0.25rem; }
.commit-meta { color: var(--muted); font-size: 12px; }

/* Report Page */
.report-page { height: 100vh; display: flex; flex-direction: column; }

.report-nav {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem 1rem;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  gap: 1rem;
  flex-wrap: wrap;
}
.nav-left, .nav-right { display: flex; align-items: center; gap: 0.5rem; }
.nav-home { color: var(--accent); text-decoration: none; font-size: 14px; }
.nav-home:hover { text-decoration: underline; }
.nav-sep { color: var(--muted); }

.op-toggle { display: flex; border: 1px solid var(--border); border-radius: 4px; overflow: hidden; }
.op-toggle button {
  padding: 0.25rem 0.75rem;
  background: var(--panel);
  border: none;
  color: var(--text);
  cursor: pointer;
  font-size: 13px;
}
.op-toggle button:first-child { border-right: 1px solid var(--border); }
.op-toggle button.active { background: var(--accent); color: white; }
.op-toggle button:hover:not(.active) { background: var(--panel2); }

.report-layout { flex: 1; display: flex; overflow: hidden; }

.report-sidebar {
  width: 200px;
  border-right: 1px solid var(--border);
  overflow-y: auto;
  padding: 1rem 0;
  background: var(--panel);
  flex-shrink: 0;
}
.sidebar-group { margin-bottom: 1rem; }
.group-label { padding: 0.25rem 1rem; font-weight: 600; font-size: 11px; text-transform: uppercase; color: var(--muted); }
.sidebar-case {
  display: block;
  width: 100%;
  text-align: left;
  padding: 0.4rem 1rem;
  background: none;
  border: none;
  color: var(--text);
  cursor: pointer;
  font-size: 13px;
}
.sidebar-case:hover { background: var(--panel2); }
.sidebar-case.active { background: var(--accent); color: white; }

.report-main { flex: 1; overflow-y: auto; padding: 1.5rem; }

.case-view { max-width: 900px; }
.case-title { margin-bottom: 1.5rem; font-size: 1.5rem; }

.results-table { width: 100%; border-collapse: collapse; margin-bottom: 1.5rem; }
.results-table th, .results-table td { padding: 0.5rem 1rem; text-align: left; border-bottom: 1px solid var(--border); }
.results-table th { font-weight: 600; font-size: 12px; text-transform: uppercase; color: var(--muted); }
.baseline-row { background: var(--panel2); }
.target-cell { }
.target-label { font-weight: 500; }
.baseline-tag { font-size: 10px; background: var(--accent); color: white; padding: 1px 4px; border-radius: 3px; margin-left: 0.5rem; }
.value-cell { font-variant-numeric: tabular-nums; }
.delta-cell { font-variant-numeric: tabular-nums; font-weight: 500; }
.error-value { color: var(--bad); font-style: italic; }

.metrics-detail { margin-top: 1.5rem; }
.metrics-detail summary { cursor: pointer; font-weight: 600; padding: 0.5rem 0; }
.metrics-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 1rem; margin-top: 1rem; }
.metrics-card { background: var(--panel); border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }
.metrics-card-header { padding: 0.5rem 0.75rem; background: var(--panel2); font-weight: 600; font-size: 13px; border-bottom: 1px solid var(--border); }
.metrics-card-body { padding: 0.5rem 0.75rem; }
.metric-row { display: flex; justify-content: space-between; padding: 0.25rem 0; font-size: 13px; }
.metric-label { color: var(--muted); }
.metric-value { font-variant-numeric: tabular-nums; }

/* Dropdown */
.dropdown { position: relative; display: inline-block; }
.dropdown-trigger {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.25rem 0.75rem;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text);
  cursor: pointer;
  font-family: var(--mono);
  font-size: 13px;
}
.dropdown-trigger:hover { background: var(--panel2); }
.dropdown-arrow { font-size: 10px; color: var(--muted); }
.dropdown-menu {
  position: absolute;
  top: 100%;
  left: 0;
  min-width: 200px;
  max-height: 300px;
  overflow-y: auto;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.15);
  z-index: 100;
  margin-top: 4px;
}
.dropdown-item {
  display: flex;
  justify-content: space-between;
  width: 100%;
  padding: 0.5rem 0.75rem;
  background: none;
  border: none;
  color: var(--text);
  cursor: pointer;
  text-align: left;
  font-size: 13px;
}
.dropdown-item:hover { background: var(--panel2); }
.dropdown-item.active { background: var(--accent); color: white; }
.dropdown-label { }
.dropdown-meta { color: var(--muted); font-size: 12px; }
.dropdown-item.active .dropdown-meta { color: rgba(255,255,255,0.8); }

/* Mobile */
@media (max-width: 768px) {
  .report-sidebar { display: none; }
  .report-nav { flex-direction: column; align-items: stretch; }
  .nav-left, .nav-right { justify-content: center; flex-wrap: wrap; }
}
`;

// ============================================================================
// Bootstrap
// ============================================================================

const styleEl = document.createElement('style');
styleEl.textContent = styles;
document.head.appendChild(styleEl);

render(html`<${App} />`, document.getElementById('app'));
