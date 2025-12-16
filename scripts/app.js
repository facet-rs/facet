// Single-page app for perf.facet.rs
// Hash-based routing: #/ = branch overview, #/branch/:name = detail

import { h, render } from 'https://esm.sh/preact@10.19.3';
import { useState, useEffect } from 'https://esm.sh/preact@10.19.3/hooks';
import htm from 'https://esm.sh/htm@3.1.1';

const html = htm.bind(h);

function formatNumber(n) {
  return n.toLocaleString();
}

function formatDelta(delta) {
  if (delta === 0) return { text: '—', color: 'var(--muted)', icon: '●' };
  const sign = delta > 0 ? '+' : '';
  const pct = `${sign}${delta.toFixed(1)}%`;
  const color = delta < 0 ? 'var(--green)' : 'var(--red)';
  const icon = delta < 0 ? '▲' : '▼';
  return { text: pct, color, icon };
}

function formatRelativeTime(iso) {
  if (!iso) return '—';
  try {
    const date = new Date(iso);
    const now = Date.now();
    const diffMs = now - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);

    if (diffSec < 60) return 'just now';
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHour < 24) return `${diffHour}h ago`;
    if (diffDay < 30) return `${diffDay}d ago`;

    const diffMonth = Math.floor(diffDay / 30);
    if (diffMonth < 12) return `${diffMonth}mo ago`;

    const diffYear = Math.floor(diffDay / 365);
    return `${diffYear}y ago`;
  } catch (e) {
    return iso;
  }
}

function formatAbsoluteTime(iso) {
  if (!iso) return '';
  try {
    const date = new Date(iso);
    return new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    }).format(date);
  } catch (e) {
    return iso;
  }
}

// Calculate delta vs main baseline
function calculateDeltaVsMain(branchInstructions, mainInstructions) {
  if (!branchInstructions || !mainInstructions) return null;
  return ((branchInstructions - mainInstructions) / mainInstructions) * 100;
}

// Branch row component with inline expansion
function BranchRow({ branch, mainLatest, expanded, onToggle }) {
  const latest = branch.commits[0];
  const delta = calculateDeltaVsMain(latest?.total_instructions, mainLatest?.total_instructions);
  const deltaInfo = delta !== null ? formatDelta(delta) : { text: '—', color: 'var(--muted)', icon: '●' };

  // Extract description from branch name or PR
  const description = latest?.pr_number
    ? `PR #${latest.pr_number}`
    : branch.name.replace(/-/g, ' ');

  return html`
    <div class="branch-row ${expanded ? 'expanded' : ''}" onClick=${onToggle}>
      <div class="branch-row-main">
        <div class="branch-info">
          <div class="branch-name">${branch.name}</div>
          <div class="branch-desc">${description}</div>
          <div class="branch-meta">
            <span class="meta-item">
              ${branch.commits.length} commit${branch.commits.length !== 1 ? 's' : ''}
            </span>
            ${latest?.timestamp && html`
              <span class="meta-item" title=${formatAbsoluteTime(latest.timestamp)}>
                ${formatRelativeTime(latest.timestamp)}
              </span>
            `}
          </div>
        </div>

        <div class="branch-delta">
          <span class="delta-icon" style="color: ${deltaInfo.color}">
            ${deltaInfo.icon}
          </span>
          <span class="delta-value" style="color: ${deltaInfo.color}">
            ${deltaInfo.text}
          </span>
        </div>
      </div>

      ${expanded && html`
        <div class="branch-expanded" onClick=${(e) => e.stopPropagation()}>
          <div class="expanded-header">Latest commit performance vs main</div>
          ${latest?.total_instructions ? html`
            <div class="perf-detail">
              <div class="perf-row">
                <span class="perf-label">Total instructions:</span>
                <span class="perf-value">${formatNumber(latest.total_instructions)}</span>
              </div>
              ${mainLatest?.total_instructions && html`
                <div class="perf-row">
                  <span class="perf-label">Main baseline:</span>
                  <span class="perf-value">${formatNumber(mainLatest.total_instructions)}</span>
                </div>
              `}
            </div>
          ` : html`
            <div class="no-data">No performance data available</div>
          `}

          <div class="expanded-links">
            <a
              href="/${branch.name}/${latest.commit}/report-deser.html"
              onClick=${(e) => e.stopPropagation()}
            >
              View full benchmark (deserialize)
            </a>
            <span style="color: var(--muted)"> | </span>
            <a
              href="/${branch.name}/${latest.commit}/report-ser.html"
              onClick=${(e) => e.stopPropagation()}
            >
              serialize
            </a>
          </div>

          ${branch.commits.length > 1 && html`
            <details class="commit-history">
              <summary>Commit history (${branch.commits.length})</summary>
              <div class="commit-list">
                ${branch.commits.slice(0, 10).map(commit => html`
                  <div key=${commit.commit} class="commit-item">
                    <a
                      href="https://github.com/facet-rs/facet/commit/${commit.commit}"
                      target="_blank"
                      onClick=${(e) => e.stopPropagation()}
                    >
                      ${commit.commit_short}
                    </a>
                    ${commit.timestamp && html`
                      <span class="commit-time" title=${formatAbsoluteTime(commit.timestamp)}>
                        ${formatRelativeTime(commit.timestamp)}
                      </span>
                    `}
                  </div>
                `)}
              </div>
            </details>
          `}
        </div>
      `}
    </div>
  `;
}

// Main branch overview page
function BranchOverview({ data }) {
  const [filter, setFilter] = useState('');
  const [showRegressionsOnly, setShowRegressionsOnly] = useState(false);
  const [expandedBranch, setExpandedBranch] = useState(null);

  const mainBranch = data.branches.main?.[0];

  // Get all branches except main
  const branches = Object.keys(data.branches)
    .filter(name => name !== 'main')
    .map(name => ({
      name,
      commits: data.branches[name]
    }))
    .filter(b => {
      // Filter by search
      if (filter && !b.name.toLowerCase().includes(filter.toLowerCase())) {
        return false;
      }

      // Filter by regressions only
      if (showRegressionsOnly) {
        const latest = b.commits[0];
        const delta = calculateDeltaVsMain(latest?.total_instructions, mainBranch?.total_instructions);
        return delta !== null && delta > 0;
      }

      return true;
    })
    .sort((a, b) => {
      // Sort by delta (best improvements first)
      const deltaA = calculateDeltaVsMain(a.commits[0]?.total_instructions, mainBranch?.total_instructions) || 0;
      const deltaB = calculateDeltaVsMain(b.commits[0]?.total_instructions, mainBranch?.total_instructions) || 0;
      return deltaA - deltaB;
    });

  return html`
    <div class="page-header">
      <div class="header-title">
        <h1>facet performance benchmarks</h1>
        <p class="header-subtitle">Comparing branches against main</p>
      </div>

      <div class="header-controls">
        <input
          type="text"
          class="filter-input"
          placeholder="Filter branches..."
          value=${filter}
          onInput=${(e) => setFilter(e.target.value)}
        />

        <label class="toggle-label">
          <input
            type="checkbox"
            checked=${showRegressionsOnly}
            onChange=${(e) => setShowRegressionsOnly(e.target.checked)}
          />
          <span>Regressions only</span>
        </label>
      </div>
    </div>

    ${mainBranch && html`
      <div class="main-baseline">
        <div class="baseline-label">Main baseline:</div>
        <div class="baseline-value">
          ${mainBranch.total_instructions ? formatNumber(mainBranch.total_instructions) : '—'} instructions
        </div>
        <div class="baseline-meta">
          ${mainBranch.timestamp && html`
            <span title=${formatAbsoluteTime(mainBranch.timestamp)}>
              updated ${formatRelativeTime(mainBranch.timestamp)}
            </span>
          `}
          <a href="/main/${mainBranch.commit}/report-deser.html">view report</a>
        </div>
      </div>
    `}

    <div class="branch-list">
      ${branches.length === 0 ? html`
        <div class="no-results">
          ${filter ? 'No branches match your filter' : 'No branches found'}
        </div>
      ` : branches.map(branch => html`
        <${BranchRow}
          key=${branch.name}
          branch=${branch}
          mainLatest=${mainBranch}
          expanded=${expandedBranch === branch.name}
          onToggle=${() => setExpandedBranch(expandedBranch === branch.name ? null : branch.name)}
        />
      `)}
    </div>
  `;
}

// Main app with routing
function App() {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    async function loadData() {
      try {
        const response = await fetch('/index.json');
        if (!response.ok) {
          throw new Error('Failed to load index.json');
        }
        const json = await response.json();
        setData(json);
      } catch (e) {
        setError(e.message);
      } finally {
        setLoading(false);
      }
    }
    loadData();
  }, []);

  if (loading) {
    return html`
      <div class="loading">Loading...</div>
    `;
  }

  if (error) {
    return html`
      <div class="error">Error: ${error}</div>
    `;
  }

  return html`<${BranchOverview} data=${data} />`;
}

// Styles
const styles = `
@font-face {
  font-family: 'Iosevka FTL';
  src: url('/fonts/IosevkaFtl-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'Iosevka FTL';
  src: url('/fonts/IosevkaFtl-Bold.ttf') format('truetype');
  font-weight: 600 700;
  font-style: normal;
  font-display: swap;
}

:root {
  --mono: 'Iosevka FTL', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  --bg: light-dark(#f8f9fa, #0d1117);
  --panel: light-dark(#ffffff, #161b22);
  --panel2: light-dark(#f6f8fa, #1c2128);
  --border: light-dark(#d0d7de, #30363d);
  --text: light-dark(#1f2328, #e6edf3);
  --muted: light-dark(#656d76, #7d8590);
  --accent: light-dark(#0969da, #58a6ff);
  --green: light-dark(#1a7f37, #3fb950);
  --red: light-dark(#cf222e, #f85149);
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: var(--mono);
  background: var(--bg);
  color: var(--text);
  font-size: 13px;
  line-height: 1.5;
}

.page-header {
  max-width: 1200px;
  margin: 0 auto;
  padding: 2rem 1rem 1.5rem;
  border-bottom: 1px solid var(--border);
}

.header-title h1 {
  font-size: 24px;
  font-weight: 600;
  margin-bottom: 0.25rem;
  letter-spacing: -0.02em;
}

.header-subtitle {
  color: var(--muted);
  font-size: 13px;
  margin-bottom: 1rem;
}

.header-controls {
  display: flex;
  gap: 1rem;
  align-items: center;
  flex-wrap: wrap;
}

.filter-input {
  flex: 1;
  min-width: 200px;
  max-width: 300px;
  padding: 0.4rem 0.75rem;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text);
  font-family: var(--mono);
  font-size: 13px;
}

.filter-input:focus {
  outline: 2px solid var(--accent);
  outline-offset: 0;
  border-color: var(--accent);
}

.toggle-label {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  cursor: pointer;
  user-select: none;
  color: var(--text);
}

.toggle-label input[type="checkbox"] {
  cursor: pointer;
}

.main-baseline {
  max-width: 1200px;
  margin: 0 auto;
  padding: 1rem 1rem;
  background: var(--panel2);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  gap: 1rem;
}

.baseline-label {
  font-weight: 600;
  color: var(--muted);
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.baseline-value {
  font-weight: 600;
  font-size: 14px;
}

.baseline-meta {
  margin-left: auto;
  color: var(--muted);
  font-size: 12px;
  display: flex;
  gap: 1rem;
}

.baseline-meta a {
  color: var(--accent);
  text-decoration: none;
}

.baseline-meta a:hover {
  text-decoration: underline;
}

.branch-list {
  max-width: 1200px;
  margin: 0 auto;
}

.branch-row {
  border-bottom: 1px solid var(--border);
  cursor: pointer;
  transition: background 0.1s;
}

.branch-row:hover {
  background: var(--panel2);
}

.branch-row.expanded {
  background: var(--panel);
}

.branch-row-main {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  gap: 2rem;
}

.branch-info {
  flex: 1;
  min-width: 0;
}

.branch-name {
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 0.1rem;
}

.branch-desc {
  color: var(--muted);
  font-size: 12px;
  margin-bottom: 0.3rem;
}

.branch-meta {
  display: flex;
  gap: 1rem;
  font-size: 11px;
  color: var(--muted);
}

.meta-item {
  cursor: help;
}

.branch-delta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-weight: 600;
  font-size: 18px;
  white-space: nowrap;
}

.delta-icon {
  font-size: 20px;
}

.branch-expanded {
  padding: 0 1rem 1rem 1rem;
  border-top: 1px solid var(--border);
  background: var(--panel);
}

.expanded-header {
  font-weight: 600;
  font-size: 12px;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 1rem 0 0.75rem;
}

.perf-detail {
  background: var(--panel2);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 0.75rem;
  margin-bottom: 0.75rem;
}

.perf-row {
  display: flex;
  justify-content: space-between;
  padding: 0.25rem 0;
}

.perf-label {
  color: var(--muted);
  font-size: 12px;
}

.perf-value {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.no-data {
  padding: 1rem;
  text-align: center;
  color: var(--muted);
  font-size: 12px;
}

.expanded-links {
  margin-bottom: 1rem;
  font-size: 12px;
}

.expanded-links a {
  color: var(--accent);
  text-decoration: none;
}

.expanded-links a:hover {
  text-decoration: underline;
}

.commit-history {
  margin-top: 0.75rem;
}

.commit-history summary {
  cursor: pointer;
  font-size: 12px;
  color: var(--muted);
  padding: 0.5rem 0;
  user-select: none;
}

.commit-history summary:hover {
  color: var(--text);
}

.commit-list {
  margin-top: 0.5rem;
  padding-left: 1rem;
}

.commit-item {
  display: flex;
  gap: 1rem;
  padding: 0.25rem 0;
  font-size: 12px;
}

.commit-item a {
  color: var(--accent);
  text-decoration: none;
  font-family: var(--mono);
}

.commit-item a:hover {
  text-decoration: underline;
}

.commit-time {
  color: var(--muted);
  cursor: help;
}

.no-results {
  padding: 3rem 1rem;
  text-align: center;
  color: var(--muted);
}

.loading, .error {
  max-width: 1200px;
  margin: 2rem auto;
  padding: 2rem 1rem;
  text-align: center;
  color: var(--muted);
}

.error {
  color: var(--red);
}

a {
  color: var(--accent);
  text-decoration: none;
}

a:hover {
  text-decoration: underline;
}
`;

// Add styles
const styleEl = document.createElement('style');
styleEl.textContent = styles;
document.head.appendChild(styleEl);

// Render the app
render(html`<${App} />`, document.getElementById('app'));
