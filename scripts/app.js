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
  const EPSILON = 0.5; // 0.5% threshold for "stalemate"

  if (Math.abs(delta) < EPSILON) {
    // Stalemate: within measurement noise
    const sign = delta > 0 ? '+' : delta < 0 ? '' : '';
    return {
      text: `${sign}${delta.toFixed(1)}%`,
      color: 'var(--neutral)',
      icon: '▬',
      label: 'neutral'
    };
  }

  const sign = delta > 0 ? '+' : '';
  const pct = `${sign}${delta.toFixed(1)}%`;
  // Negative delta = fewer instructions = faster = good (green)
  // Positive delta = more instructions = slower = bad (red)
  const color = delta < 0 ? 'var(--good)' : 'var(--bad)';
  const icon = delta < 0 ? '▲' : '▼';
  const label = delta < 0 ? 'faster' : 'slower';
  return { text: pct, color, icon, label };
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

// Calculate delta vs main baseline (for ratios)
function calculateDeltaVsMain(branchRatio, mainRatio) {
  if (!branchRatio || !mainRatio) return null;
  return ((branchRatio - mainRatio) / mainRatio) * 100;
}

// Branch row component with inline expansion
function BranchRow({ branch, baseline, expanded, onToggle }) {
  const latest = branch.commits[0];
  const delta = baseline.state !== 'none'
    ? calculateDeltaVsMain(latest?.facet_vs_serde_ratio, baseline.ratio)
    : null;
  const deltaInfo = delta !== null ? formatDelta(delta) : { text: '—', color: 'var(--muted)', icon: '●' };

  // Subject: commit message (first line) - the actual change description
  let subject = '';
  if (latest?.commit_message && latest.commit_message.trim()) {
    subject = latest.commit_message.split('\n')[0].trim();
  } else if (latest?.commit_short) {
    subject = `(no message)`;
  }

  return html`
    <div class="branch-row ${expanded ? 'expanded' : ''}" onClick=${onToggle}>
      <div class="branch-row-main">
        <div class="branch-info">
          <div class="branch-name">${branch.name}</div>
          <div class="branch-subject">${subject}</div>
          <div class="branch-meta">
            <span class="meta-item">
              ${branch.commits.length} commit${branch.commits.length !== 1 ? 's' : ''}
            </span>
            ${latest?.timestamp && html`
              <span class="meta-item" title=${formatAbsoluteTime(latest.timestamp)}>
                last run ${formatRelativeTime(latest.timestamp)}
              </span>
            `}
          </div>
        </div>

        <div class="branch-result">
          ${latest?.facet_vs_serde_ratio && html`
            <span class="result-value">
              ${(latest.facet_vs_serde_ratio * 100).toFixed(1)}% of serde
            </span>
          `}
          ${baseline.state !== 'none' && delta !== null && html`
            <span class="result-delta" style="color: ${deltaInfo.color}">
              ${deltaInfo.icon} ${deltaInfo.text}
            </span>
          `}
        </div>
      </div>

      ${expanded && html`
        <div class="branch-expanded" onClick=${(e) => e.stopPropagation()}>
          <!-- Expanded header: re-anchor the eye -->
          <div class="expanded-branch-header">
            <div class="expanded-branch-name">${branch.name}</div>
            <div class="expanded-branch-subject">${subject}</div>
            <div class="expanded-branch-meta">
              <span>last run ${formatRelativeTime(latest.timestamp)}</span>
              <span>·</span>
              <span>latest commit: ${latest.commit_short}</span>
              <span>·</span>
              <span>${branch.commits.length} commit${branch.commits.length !== 1 ? 's' : ''}</span>
            </div>
          </div>

          <!-- Result summary block -->
          <div class="result-summary">
            <div class="result-summary-header">
              ${baseline.state === 'real' ? 'Latest result vs main' :
                baseline.state === 'estimated' ? 'Latest result vs estimated reference' :
                'Latest result'}
            </div>

            ${latest?.facet_vs_serde_ratio ? html`
              <div class="result-summary-content">
                <div class="result-primary">
                  ${(latest.facet_vs_serde_ratio * 100).toFixed(1)}% of serde_json
                </div>
                ${baseline.state === 'real' && delta !== null && html`
                  <div class="result-delta-large" style="color: ${deltaInfo.color}">
                    ${deltaInfo.icon} ${deltaInfo.text}
                  </div>
                `}
                ${baseline.state === 'estimated' && delta !== null && html`
                  <div class="result-delta-estimated" style="color: ${deltaInfo.color}">
                    ${deltaInfo.icon} ${deltaInfo.text} <span class="estimated-tag">(estimated)</span>
                  </div>
                `}
              </div>
            ` : html`
              <div class="no-data">No performance data available</div>
            `}

            <div class="result-links">
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
          </div>

          <!-- Commit history block -->
          ${branch.commits.length > 1 && html`
            <details class="commit-history">
              <summary>Commit history (${branch.commits.length})</summary>
              <div class="commit-list">
                ${branch.commits.slice(0, 10).map(commit => {
                  const commitSubject = commit.commit_message
                    ? commit.commit_message.split('\n')[0].trim()
                    : '(no message)';
                  const reportUrl = `/${branch.name}/${commit.commit}/report-deser.html`;

                  // Calculate delta for this commit vs baseline
                  const commitDelta = baseline.state !== 'none'
                    ? calculateDeltaVsMain(commit?.facet_vs_serde_ratio, baseline.ratio)
                    : null;
                  const deltaInfo = commitDelta !== null ? formatDelta(commitDelta) : null;

                  return html`
                    <a
                      key=${commit.commit}
                      class="commit-item"
                      href=${reportUrl}
                      onClick=${(e) => e.stopPropagation()}
                    >
                      <div class="commit-subject">${commitSubject}</div>
                      <div class="commit-meta-line">
                        <span class="commit-hash">${commit.commit_short}</span>
                        ${commit.timestamp && html`
                          <span class="commit-time" title=${formatAbsoluteTime(commit.timestamp)}>
                            · ${formatRelativeTime(commit.timestamp)}
                          </span>
                        `}
                        ${deltaInfo && html`
                          <span class="commit-delta" style="color: ${deltaInfo.color}">
                            · ${deltaInfo.icon} ${deltaInfo.text}
                          </span>
                        `}
                      </div>
                    </a>
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

// Determine baseline state honestly
function getBaselineState(data) {
  const mainCommit = data.branches.main?.[0];

  // Real baseline: main has actual data
  if (mainCommit && mainCommit.facet_vs_serde_ratio) {
    return {
      state: 'real',
      ratio: mainCommit.facet_vs_serde_ratio,
      instructions: mainCommit.total_instructions,
      commit: mainCommit.commit,
      timestamp: mainCommit.timestamp
    };
  }

  // Estimated baseline: use median of other branches
  const allRatios = Object.values(data.branches)
    .flat()
    .map(c => c.facet_vs_serde_ratio)
    .filter(Boolean);

  if (allRatios.length > 0) {
    allRatios.sort((a, b) => a - b);
    const median = allRatios[Math.floor(allRatios.length / 2)];
    return {
      state: 'estimated',
      ratio: median,
      instructions: null,
      commit: null,
      timestamp: null
    };
  }

  // No baseline available
  return {
    state: 'none',
    ratio: null,
    instructions: null,
    commit: null,
    timestamp: null
  };
}

// Main branch overview page
function BranchOverview({ data }) {
  const [filter, setFilter] = useState('');
  const [showRegressionsOnly, setShowRegressionsOnly] = useState(false);
  const [expandedBranch, setExpandedBranch] = useState(null);

  // Get baseline state
  const baseline = getBaselineState(data);

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
      if (showRegressionsOnly && baseline.state !== 'none') {
        const latest = b.commits[0];
        const delta = calculateDeltaVsMain(latest?.facet_vs_serde_ratio, baseline.ratio);
        return delta !== null && delta > 0;
      }

      return true;
    })
    .sort((a, b) => {
      // Sort by delta if we have a baseline
      if (baseline.state !== 'none') {
        const deltaA = calculateDeltaVsMain(a.commits[0]?.facet_vs_serde_ratio, baseline.ratio) || 0;
        const deltaB = calculateDeltaVsMain(b.commits[0]?.facet_vs_serde_ratio, baseline.ratio) || 0;
        return deltaA - deltaB;
      }
      // Otherwise sort by latest timestamp
      return (b.commits[0]?.timestamp || '').localeCompare(a.commits[0]?.timestamp || '');
    });

  return html`
    <div class="page-header">
      <div class="header-title">
        <h1>facet performance benchmarks</h1>
        <p class="header-subtitle">
          ${baseline.state === 'real' ? 'Comparing branches against main' :
            baseline.state === 'estimated' ? 'Branch performance data' :
            'Performance benchmark results'}
        </p>
      </div>

      <div class="header-controls">
        <input
          type="text"
          class="filter-input"
          placeholder="Filter branches..."
          value=${filter}
          onInput=${(e) => setFilter(e.target.value)}
        />

        ${baseline.state !== 'none' && html`
          <label class="toggle-label">
            <input
              type="checkbox"
              checked=${showRegressionsOnly}
              onChange=${(e) => setShowRegressionsOnly(e.target.checked)}
            />
            <span>Regressions only</span>
          </label>
        `}
      </div>
    </div>

    ${baseline.state === 'real' && html`
      <div class="main-baseline">
        <div class="baseline-label">Baseline: main @ ${baseline.commit?.substring(0, 7)}</div>
        <div class="baseline-value">
          ${(baseline.ratio * 100).toFixed(1)}% of serde_json
        </div>
        <div class="baseline-meta">
          <span title=${formatAbsoluteTime(baseline.timestamp)}>
            updated ${formatRelativeTime(baseline.timestamp)}
          </span>
          <a href="/main/${baseline.commit}/report-deser.html">view report</a>
        </div>
      </div>
    `}

    ${baseline.state === 'estimated' && html`
      <div class="main-baseline estimated">
        <div class="baseline-label">
          <span>REFERENCE (estimated)</span>
          <span class="info-icon" title="No main branch data available. Using median of all branch results as reference.">ⓘ</span>
        </div>
        <div class="baseline-value" style="color: var(--muted);">
          ${(baseline.ratio * 100).toFixed(1)}% of serde_json
        </div>
        <div class="baseline-meta">
          <span style="color: var(--muted); font-style: italic;">
            Derived from median of branch results
          </span>
        </div>
      </div>
    `}

    ${baseline.state === 'none' && html`
      <div class="main-baseline none">
        <div class="baseline-label">No baseline available</div>
        <div class="baseline-meta">
          <span style="color: var(--muted);">
            Run benchmarks on main to enable comparisons
          </span>
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
          baseline=${baseline}
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

// Index-specific styles (shared styles loaded from /shared-styles.css)
const styles = `

.page-header {
  max-width: 1200px;
  margin: 0 auto;
  padding: 2rem 1rem 1.5rem;
  border-bottom: 1px solid var(--border);
}

.header-title h1 {
  margin-bottom: 0.25rem;
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
  display: flex;
  align-items: center;
  gap: 0.5rem;
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

.main-baseline.estimated .baseline-label {
  color: var(--orange, #f9826c);
}

.main-baseline.none .baseline-label {
  color: var(--muted);
  text-transform: none;
  font-size: 14px;
}

.info-icon {
  cursor: help;
  font-style: normal;
  opacity: 0.7;
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
  border-left: 3px solid var(--accent);
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
  margin-bottom: 0.25rem;
}

.branch-subject {
  color: var(--muted);
  font-size: 13px;
  margin-bottom: 0.3rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.branch-result {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.25rem;
  white-space: nowrap;
}

.result-value {
  font-weight: 600;
  font-size: 14px;
  font-variant-numeric: tabular-nums;
}

.result-delta {
  font-weight: 600;
  font-size: 16px;
  font-variant-numeric: tabular-nums;
}

.branch-expanded {
  padding: 1rem 1rem 1rem 1.5rem;
  border-top: 1px solid var(--border);
  background: var(--panel);
}

/* Expanded header block */
.expanded-branch-header {
  margin-bottom: 1.25rem;
}

.expanded-branch-name {
  font-size: 20px;
  font-weight: 650;
  margin-bottom: 0.4rem;
}

.expanded-branch-subject {
  font-size: 14px;
  color: var(--text);
  margin-bottom: 0.5rem;
}

.expanded-branch-meta {
  font-size: 12px;
  color: var(--muted);
  display: flex;
  gap: 0.5rem;
}

/* Result summary block */
.result-summary {
  margin-bottom: 1.25rem;
}

.result-summary-header {
  font-weight: 600;
  font-size: 12px;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.75rem;
}

.result-summary-content {
  background: var(--panel2);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 1rem;
  margin-bottom: 0.75rem;
}

.result-primary {
  font-size: 16px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  margin-bottom: 0.5rem;
}

.result-delta-large {
  font-size: 24px;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
}

.result-delta-estimated {
  font-size: 20px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.estimated-tag {
  font-size: 12px;
  font-weight: 500;
  opacity: 0.8;
}

.no-data {
  padding: 1rem;
  text-align: center;
  color: var(--muted);
  font-size: 12px;
}

.result-links {
  font-size: 12px;
}

.result-links a {
  color: var(--accent);
  text-decoration: none;
}

.result-links a:hover {
  text-decoration: underline;
}

.commit-history {
  margin-top: 1rem;
  border-top: 1px solid var(--border);
  padding-top: 0.75rem;
}

.commit-history summary {
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  padding: 0.5rem 0;
  user-select: none;
}

.commit-history summary:hover {
  color: var(--accent);
}

.commit-list {
  margin-top: 0.75rem;
  padding-left: 0.5rem;
}

.commit-item {
  display: block;
  padding: 0.6rem 0.5rem;
  border-bottom: 1px solid var(--border);
  text-decoration: none;
  transition: background 0.1s;
  border-radius: 4px;
  margin: 0 -0.5rem;
}

.commit-item:hover {
  background: var(--panel2);
}

.commit-item:last-child {
  border-bottom: none;
}

.commit-subject {
  color: var(--text);
  font-size: 13px;
  margin-bottom: 0.3rem;
  line-height: 1.4;
}

.commit-meta-line {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  font-size: 12px;
  color: var(--muted);
}

.commit-hash {
  color: var(--muted);
  font-family: var(--mono);
}

.commit-time {
  color: var(--muted);
  font-size: 11px;
  cursor: help;
}

.commit-delta {
  font-size: 11px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
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
  color: var(--bad);
}
`;

// Add styles
const styleEl = document.createElement('style');
styleEl.textContent = styles;
document.head.appendChild(styleEl);

// Render the app
render(html`<${App} />`, document.getElementById('app'));
