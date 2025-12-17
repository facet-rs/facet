// Single-page app for perf.facet.rs
// Supports both index-v2.json (commit-centric) and legacy index.json (branch-first)

import { h, render } from 'https://esm.sh/preact@10.19.3';
import { useState, useEffect, useCallback } from 'https://esm.sh/preact@10.19.3/hooks';
import htm from 'https://esm.sh/htm@3.1.1';

const html = htm.bind(h);

// Cache for run.json fetches
const runCache = new Map();

function formatNumber(n) {
  return n.toLocaleString();
}

function formatDelta(delta) {
  const EPSILON = 0.5; // 0.5% threshold for "stalemate"

  if (Math.abs(delta) < EPSILON) {
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

function formatRelativeTime(input) {
  if (!input) return '—';
  try {
    // Handle both ISO string and Unix timestamp
    const date = typeof input === 'number' ? new Date(input * 1000) : new Date(input);
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
    return String(input);
  }
}

function formatAbsoluteTime(input) {
  if (!input) return '';
  try {
    const date = typeof input === 'number' ? new Date(input * 1000) : new Date(input);
    return new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    }).format(date);
  } catch (e) {
    return String(input);
  }
}

// Fetch run.json and compute headline metrics
async function fetchRunData(runJsonUrl) {
  if (runCache.has(runJsonUrl)) {
    return runCache.get(runJsonUrl);
  }

  try {
    const response = await fetch(runJsonUrl);
    if (!response.ok) return null;
    const data = await response.json();
    runCache.set(runJsonUrl, data);
    return data;
  } catch (e) {
    console.error(`Failed to fetch ${runJsonUrl}:`, e);
    return null;
  }
}

// Compute facet vs serde ratio from run.json results
function computeRatio(runData, operation = 'deserialize', metric = 'instructions') {
  if (!runData?.results) return null;

  let facetTotal = 0;
  let serdeTotal = 0;

  for (const [caseName, caseData] of Object.entries(runData.results)) {
    const targets = caseData?.targets || {};

    // Find facet_format_jit result
    const facetResult = targets['facet_format_jit']?.ops?.[operation];
    if (facetResult?.ok && facetResult.metrics?.[metric]) {
      facetTotal += facetResult.metrics[metric];
    }

    // Find serde_json result
    const serdeResult = targets['serde_json']?.ops?.[operation];
    if (serdeResult?.ok && serdeResult.metrics?.[metric]) {
      serdeTotal += serdeResult.metrics[metric];
    }
  }

  if (serdeTotal > 0) {
    return facetTotal / serdeTotal;
  }
  return null;
}

// Transform index-v2.json to normalized format for UI
function normalizeIndexData(data) {
  // Check if this is index-v2 format
  if (data.version === 2) {
    return normalizeV2Data(data);
  }

  // Legacy format - already has branches with commit arrays
  return normalizeLegacyData(data);
}

function normalizeV2Data(data) {
  // Transform commit-centric index to branch-first for UI
  const branches = {};

  for (const [branchKey, branchInfo] of Object.entries(data.branches || {})) {
    const branchCommits = data.branch_commits?.[branchKey] || [];
    const commits = branchCommits.map(ref => {
      const commitData = data.commits?.[ref.sha] || {};
      const run = commitData.runs?.[branchKey] || {};

      return {
        commit: ref.sha,
        commit_short: ref.short,
        timestamp: run.timestamp || null,
        timestamp_unix: ref.timestamp_unix,
        commit_message: run.commit_message || commitData.subject || '',
        pr_title: run.pr_title || '',
        pr_number: run.pr_number || branchInfo.pr_number || null,
        // These will be lazy-loaded from run.json
        facet_vs_serde_ratio: null,
        total_instructions: null,
        run_json_url: ref.run_json_url,
        // Flag to indicate we need to fetch details
        needsLoad: true
      };
    });

    branches[branchKey] = commits;
  }

  return {
    branches,
    baseline: data.baseline,
    defaults: data.defaults,
    metric_specs: data.metric_specs,
    version: 2
  };
}

function normalizeLegacyData(data) {
  // Legacy data already has the right structure
  return {
    branches: data.branches || {},
    baseline: null,
    defaults: null,
    metric_specs: null,
    version: 1
  };
}

// Calculate delta vs main baseline (for ratios)
function calculateDeltaVsMain(branchRatio, mainRatio) {
  if (!branchRatio || !mainRatio) return null;
  return ((branchRatio - mainRatio) / mainRatio) * 100;
}

// Branch row component with inline expansion
function BranchRow({ branch, branchKey, baseline, expanded, onToggle, onLoadHeadline }) {
  const latest = branch.commits[0];
  const [ratio, setRatio] = useState(latest?.facet_vs_serde_ratio);
  const [loading, setLoading] = useState(false);

  // Load headline data when expanded or on mount
  useEffect(() => {
    if (latest?.needsLoad && latest?.run_json_url && !ratio && !loading) {
      setLoading(true);
      fetchRunData(latest.run_json_url).then(runData => {
        if (runData) {
          const computed = computeRatio(runData);
          setRatio(computed);
          // Update parent data
          if (onLoadHeadline && computed !== null) {
            onLoadHeadline(branchKey, latest.commit, computed);
          }
        }
        setLoading(false);
      });
    }
  }, [latest?.run_json_url, ratio, loading]);

  const effectiveRatio = ratio || latest?.facet_vs_serde_ratio;
  const delta = baseline.state !== 'none'
    ? calculateDeltaVsMain(effectiveRatio, baseline.ratio)
    : null;
  const deltaInfo = delta !== null ? formatDelta(delta) : { text: '—', color: 'var(--muted)', icon: '●' };

  // Subject: commit message (first line) - the actual change description
  let subject = '';
  if (latest?.pr_title && latest.pr_title.trim()) {
    subject = latest.pr_title.trim();
  } else if (latest?.commit_message && latest.commit_message.trim()) {
    subject = latest.commit_message.split('\n')[0].trim();
  } else if (latest?.commit_short) {
    subject = '(no message)';
  }

  // Build report URL - use new path for v2, old path for legacy
  const reportBasePath = latest?.run_json_url
    ? latest.run_json_url.replace('/run.json', '')
    : `/${branchKey}/${latest?.commit}`;

  return html`
    <div class="branch-row ${expanded ? 'expanded' : ''}" onClick=${onToggle}>
      <div class="branch-row-main">
        <div class="branch-info">
          <div class="branch-name">${branchKey}</div>
          <div class="branch-subject">${subject}</div>
          <div class="branch-meta">
            <span class="meta-item">
              ${branch.commits.length} commit${branch.commits.length !== 1 ? 's' : ''}
            </span>
            ${(latest?.timestamp || latest?.timestamp_unix) && html`
              <span class="meta-item" title=${formatAbsoluteTime(latest.timestamp || latest.timestamp_unix)}>
                last run ${formatRelativeTime(latest.timestamp || latest.timestamp_unix)}
              </span>
            `}
          </div>
        </div>

        <div class="branch-result">
          ${loading && html`<span class="result-loading">Loading...</span>`}
          ${effectiveRatio && html`
            <span class="result-value">
              ${(effectiveRatio * 100).toFixed(1)}% of serde
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
          <div class="expanded-branch-header">
            <div class="expanded-branch-name">${branchKey}</div>
            <div class="expanded-branch-subject">${subject}</div>
            <div class="expanded-branch-meta">
              <span>last run ${formatRelativeTime(latest?.timestamp || latest?.timestamp_unix)}</span>
              <span>·</span>
              <span>latest commit: ${latest?.commit_short}</span>
              <span>·</span>
              <span>${branch.commits.length} commit${branch.commits.length !== 1 ? 's' : ''}</span>
            </div>
          </div>

          <div class="result-summary">
            <div class="result-summary-header">
              ${baseline.state === 'real' ? 'Latest result vs main' :
                baseline.state === 'estimated' ? 'Latest result vs estimated reference' :
                'Latest result'}
            </div>

            ${effectiveRatio ? html`
              <div class="result-summary-content">
                <div class="result-primary">
                  ${(effectiveRatio * 100).toFixed(1)}% of serde_json
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
              <div class="no-data">
                ${loading ? 'Loading performance data...' : 'No performance data available'}
              </div>
            `}

            <div class="result-links">
              <a
                href="${reportBasePath}/report-deser.html"
                onClick=${(e) => e.stopPropagation()}
              >
                View full benchmark (deserialize)
              </a>
              <span style="color: var(--muted)"> | </span>
              <a
                href="${reportBasePath}/report-ser.html"
                onClick=${(e) => e.stopPropagation()}
              >
                serialize
              </a>
            </div>
          </div>

          ${branch.commits.length > 1 && html`
            <${CommitHistory}
              commits=${branch.commits}
              branchKey=${branchKey}
              baseline=${baseline}
            />
          `}
        </div>
      `}
    </div>
  `;
}

// Commit history component
function CommitHistory({ commits, branchKey, baseline }) {
  return html`
    <details class="commit-history">
      <summary>Commit history (${commits.length})</summary>
      <div class="commit-list">
        ${commits.slice(0, 10).map(commit => {
          const commitSubject = commit.pr_title?.trim()
            || commit.commit_message?.split('\n')[0]?.trim()
            || '(no message)';

          const reportBasePath = commit.run_json_url
            ? commit.run_json_url.replace('/run.json', '')
            : `/${branchKey}/${commit.commit}`;
          const reportUrl = `${reportBasePath}/report-deser.html`;

          // Calculate delta for this commit vs baseline
          const commitDelta = baseline.state !== 'none' && commit.facet_vs_serde_ratio
            ? calculateDeltaVsMain(commit.facet_vs_serde_ratio, baseline.ratio)
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
                ${(commit.timestamp || commit.timestamp_unix) && html`
                  <span class="commit-time" title=${formatAbsoluteTime(commit.timestamp || commit.timestamp_unix)}>
                    · ${formatRelativeTime(commit.timestamp || commit.timestamp_unix)}
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
  `;
}

// Determine baseline state
function getBaselineState(data) {
  // For v2 format, use the baseline from the index
  if (data.version === 2 && data.baseline) {
    // We'll need to fetch the actual ratio from run.json
    return {
      state: 'loading',
      ratio: null,
      commit: data.baseline.commit_sha,
      timestamp: data.baseline.timestamp,
      run_json_url: data.baseline.run_json_url
    };
  }

  // Legacy format - check main branch directly
  const mainCommits = data.branches?.main;
  const mainCommit = mainCommits?.[0];

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
  const allRatios = Object.values(data.branches || {})
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
  const [baseline, setBaseline] = useState(() => getBaselineState(data));
  const [headlineCache, setHeadlineCache] = useState({});

  // Load baseline ratio from run.json if needed
  useEffect(() => {
    if (baseline.state === 'loading' && baseline.run_json_url) {
      fetchRunData(baseline.run_json_url).then(runData => {
        if (runData) {
          const ratio = computeRatio(runData);
          setBaseline(prev => ({
            ...prev,
            state: ratio ? 'real' : 'none',
            ratio
          }));
        } else {
          setBaseline(prev => ({ ...prev, state: 'none' }));
        }
      });
    }
  }, [baseline.state, baseline.run_json_url]);

  // Callback to update headline cache
  const handleLoadHeadline = useCallback((branchKey, commit, ratio) => {
    setHeadlineCache(prev => ({
      ...prev,
      [`${branchKey}:${commit}`]: ratio
    }));
  }, []);

  // Get all branches
  const branches = Object.keys(data.branches || {})
    .map(name => ({
      name,
      commits: data.branches[name].map(c => {
        // Merge cached headline data
        const cacheKey = `${name}:${c.commit}`;
        if (headlineCache[cacheKey]) {
          return { ...c, facet_vs_serde_ratio: headlineCache[cacheKey] };
        }
        return c;
      })
    }))
    .filter(b => {
      // Filter by search
      if (filter && !b.name.toLowerCase().includes(filter.toLowerCase())) {
        return false;
      }

      // Filter by regressions only
      if (showRegressionsOnly && baseline.state === 'real') {
        const latest = b.commits[0];
        const ratio = latest?.facet_vs_serde_ratio || headlineCache[`${b.name}:${latest?.commit}`];
        const delta = calculateDeltaVsMain(ratio, baseline.ratio);
        return delta !== null && delta > 0;
      }

      return true;
    })
    .sort((a, b) => {
      // Sort main first
      if (a.name === 'main') return -1;
      if (b.name === 'main') return 1;

      // Then by delta if we have a baseline
      if (baseline.state === 'real') {
        const ratioA = a.commits[0]?.facet_vs_serde_ratio || headlineCache[`${a.name}:${a.commits[0]?.commit}`];
        const ratioB = b.commits[0]?.facet_vs_serde_ratio || headlineCache[`${b.name}:${b.commits[0]?.commit}`];
        const deltaA = calculateDeltaVsMain(ratioA, baseline.ratio) || 0;
        const deltaB = calculateDeltaVsMain(ratioB, baseline.ratio) || 0;
        return deltaA - deltaB;
      }

      // Otherwise sort by latest timestamp
      const tsA = a.commits[0]?.timestamp_unix || a.commits[0]?.timestamp || '';
      const tsB = b.commits[0]?.timestamp_unix || b.commits[0]?.timestamp || '';
      return String(tsB).localeCompare(String(tsA));
    });

  return html`
    <div class="page-header">
      <div class="header-title">
        <h1>facet performance benchmarks</h1>
        <p class="header-subtitle">
          ${baseline.state === 'real' ? 'Comparing branches against main' :
            baseline.state === 'loading' ? 'Loading baseline...' :
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

        ${baseline.state === 'real' && html`
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
          <a href="/runs/main/${baseline.commit}/report-deser.html">view report</a>
        </div>
      </div>
    `}

    ${baseline.state === 'loading' && html`
      <div class="main-baseline loading">
        <div class="baseline-label">Loading baseline...</div>
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
          branchKey=${branch.name}
          baseline=${baseline}
          expanded=${expandedBranch === branch.name}
          onToggle=${() => setExpandedBranch(expandedBranch === branch.name ? null : branch.name)}
          onLoadHeadline=${handleLoadHeadline}
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
        // Try index-v2.json first, fall back to index.json
        let response = await fetch('/index-v2.json');
        if (!response.ok) {
          response = await fetch('/index.json');
        }
        if (!response.ok) {
          throw new Error('Failed to load index data');
        }
        const json = await response.json();
        const normalized = normalizeIndexData(json);
        setData(normalized);
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

.main-baseline.loading {
  color: var(--muted);
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

.result-loading {
  color: var(--muted);
  font-size: 12px;
  font-style: italic;
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
