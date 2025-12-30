// Unified SPA for perf.facet.rs
// Hash-based routing: /#/ (index), /#/runs/:branch/:commit/:op (report)

import { h, render } from 'https://esm.sh/preact@10.19.3';
import { useState, useEffect, useCallback, useMemo, useRef } from 'https://esm.sh/preact@10.19.3/hooks';
import { Router, Route, useParams } from 'https://esm.sh/wouter-preact@3.8.1?deps=preact@10.19.3';
import { useHashLocation } from 'https://esm.sh/wouter-preact@3.8.1/use-hash-location?deps=preact@10.19.3';
import htm from 'https://esm.sh/htm@3.1.1';

const html = htm.bind(h);

// Hash-based router wrapper
function HashRouter({ children }) {
  return html`<${Router} hook=${useHashLocation}>${children}<//>`;
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

function formatRatio(ratio) {
  if (!ratio || ratio <= 0) return '—';
  return `${ratio.toFixed(2)}×`;
}

// Format tier usage indicators for JIT targets
function getTierIndicator(targetData, targetId) {
  // Only show tier indicators for jit_t2 targets
  if (!targetId.includes('jit_t2')) return null;

  const tier2_attempts = targetData?.tier2_attempts ?? 0;
  const tier2_successes = targetData?.tier2_successes ?? 0;
  const tier1_fallbacks = targetData?.tier1_fallbacks ?? 0;

  // No tier data available
  if (tier2_attempts === 0 && tier1_fallbacks === 0) {
    return null;
  }

  // Tier-2 success
  if (tier2_successes > 0) {
    return {
      icon: '⚡',
      label: 'Tier-2',
      title: `Using Tier-2 JIT (format-specific IR, ${tier2_successes}/${tier2_attempts} successful)`,
      color: 'var(--good)'
    };
  }

  // Tier-1 fallback
  if (tier1_fallbacks > 0) {
    return {
      icon: '⚙',
      label: 'Tier-1',
      title: `Tier-2 unavailable, using Tier-1 JIT (shape-based, ${tier1_fallbacks} fallbacks)`,
      color: 'var(--warning)'
    };
  }

  return null;
}

// Format ratio vs baseline with proper semantics and epsilon for neutrality
// ratio = baseline_instructions / facet_instructions
// ratio > 1 means facet uses fewer instructions = faster
// ratio < 1 means facet uses more instructions = slower
// ratio ≈ 1 means roughly the same
function formatSpeedupVsBaseline(ratio, baselineLabel = 'baseline') {
  if (!ratio || ratio <= 0) return { text: '—', label: '', color: null };

  // Show ratio directly: 0.2× means 20% of baseline's speed, 2× means twice as fast
  // Higher is always better, no confusing "slower"/"faster" language
  const EPSILON = 0.03;

  if (Math.abs(ratio - 1) < EPSILON) {
    return { text: '~1×', label: baselineLabel, color: 'var(--neutral)' };
  }

  // Color based on whether we're faster or slower than baseline
  const color = ratio >= 1 ? 'var(--good)' : 'var(--muted)';
  return { text: `${ratio.toFixed(2)}×`, label: baselineLabel, color };
}

function formatDelta(delta) {
  // Positive delta = improvement (ratio went up = faster)
  // Negative delta = regression (ratio went down = slower)
  const EPSILON = 0.5;
  if (Math.abs(delta) < EPSILON) {
    return { text: `${delta > 0 ? '+' : ''}${delta.toFixed(1)}%`, color: 'var(--neutral)', icon: '▬' };
  }
  const sign = delta > 0 ? '+' : '';
  return {
    text: `${sign}${delta.toFixed(1)}%`,
    color: delta > 0 ? 'var(--good)' : 'var(--bad)',
    icon: delta > 0 ? '▲' : '▼'
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

function formatRatioVsBaseline(ratio) {
  if (ratio === null || ratio === undefined) return { text: '—', color: null };
  const EPSILON = 0.02; // 2% tolerance for "same"
  if (Math.abs(ratio - 1) < EPSILON) {
    return { text: '1×', color: 'var(--neutral)' };
  }
  // ratio < 1 means fewer instructions = faster = good
  // ratio > 1 means more instructions = slower = bad
  const color = ratio < 1 ? 'var(--good)' : 'var(--bad)';
  const text = ratio < 1 ? `${ratio.toFixed(2)}×` : `${ratio.toFixed(2)}×`;
  return { text, color };
}

// Find which group a benchmark belongs to
function findBenchmarkGroup(benchId, catalog) {
  if (!catalog?.groups) return 'other';

  for (const [groupId, group] of Object.entries(catalog.groups)) {
    if (group.benchmarks_order?.includes(benchId)) {
      return groupId;
    }
  }
  return 'other';
}

// ============================================================================
// Shared Components
// ============================================================================

function Link({ href, children, ...props }) {
  const [, navigate] = useHashLocation();

  const onClick = useCallback((e) => {
    if (!e.ctrlKey && !e.metaKey && !e.shiftKey) {
      e.preventDefault();
      navigate(href);
    }
  }, [href, navigate]);

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
// Index Page - Commit-Centric Timeline
// ============================================================================

function IndexPage() {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    fetchIndexData().then(d => {
      if (d) setData(d);
      else setError('Failed to load index data');
      setLoading(false);
    });
  }, []);

  if (loading) return html`<div class="loading">Loading...</div>`;
  if (error) return html`<div class="error">${error}</div>`;
  if (!data) return html`<div class="error">No data</div>`;

  const timeline = data.timeline || Object.keys(data.commits || {});
  const baseline = data.baseline;
  const baselineRatio = baseline?.headline?.ratio;

  const filteredTimeline = filter
    ? timeline.filter(sha => {
        const commit = data.commits?.[sha];
        if (!commit) return false;
        const searchLower = filter.toLowerCase();
        return (
          sha.toLowerCase().includes(searchLower) ||
          commit.short?.toLowerCase().includes(searchLower) ||
          commit.subject?.toLowerCase().includes(searchLower) ||
          commit.branches_present?.some(b => b.toLowerCase().includes(searchLower))
        );
      })
    : timeline;

  return html`
    <div class="index-page">
      <header class="page-header">
        <h1>facet performance benchmarks</h1>
        <p class="subtitle">Comparing facet vs standard libraries (instructions, deserialize)</p>
        <input
          type="text"
          class="filter-input"
          placeholder="Filter commits..."
          value=${filter}
          onInput=${(e) => setFilter(e.target.value)}
        />
      </header>

      ${baseline && baselineRatio > 0 ? (() => {
        const speedup = formatSpeedupVsBaseline(baselineRatio);
        return html`
          <div class="baseline-banner">
            <span class="baseline-label">Baseline: main</span>
            <span class="baseline-value" style=${speedup.color ? `color: ${speedup.color}` : ''}>${speedup.text} ${speedup.label}</span>
            ${baseline.commit_sha && html`
              <${Link} href="/runs/${baseline.branch_key}/${baseline.commit_sha}/deserialize" class="baseline-link">
                view report
              <//>
            `}
          </div>
        `;
      })() : null}

      <div class="commit-timeline">
        ${filteredTimeline.map(sha => {
          const commit = data.commits?.[sha];
          if (!commit) return null;
          return html`
            <${CommitRow}
              key=${sha}
              commit=${commit}
              baseline=${baseline}
              baselineRatio=${baselineRatio}
            />
          `;
        })}
        ${filteredTimeline.length === 0 && html`
          <div class="no-results">No commits match your filter</div>
        `}
      </div>
    </div>
  `;
}

function CommitRow({ commit, baseline, baselineRatio }) {
  const [expanded, setExpanded] = useState(false);

  // Use new summary structure if available, fall back to old headline
  const summary = commit.summary;
  const headline = summary?.headline || commit.headline;
  const ratio = headline?.ratio;

  // Use pre-computed delta if available, otherwise compute
  const delta = headline?.delta_vs_baseline ?? (ratio && baselineRatio
    ? ((ratio - baselineRatio) / baselineRatio) * 100
    : null);
  const deltaDirection = headline?.delta_direction;
  const deltaInfo = delta !== null ? formatDelta(delta) : null;

  // Get highlights
  const highlights = summary?.highlights;
  const regressions = highlights?.regressions || [];
  const improvements = highlights?.improvements || [];
  const hasHighlights = regressions.length > 0 || improvements.length > 0;

  const primaryBranch = commit.primary_default?.branch_key || commit.branches_present?.[0] || 'main';
  const isBaseline = baseline?.commit_sha === commit.sha;

  const run = commit.runs?.[primaryBranch];
  const runUrl = run ? `/runs/${primaryBranch}/${commit.sha}/deserialize` : null;

  return html`
    <div class="commit-row ${isBaseline ? 'is-baseline' : ''} ${expanded ? 'expanded' : ''}">
      <div class="commit-main">
        <div class="commit-info">
          <div class="commit-header">
            <span class="commit-sha">${commit.short}</span>
            <span class="commit-branches">
              ${commit.branches_present?.map(b => html`
                <span key=${b} class="branch-badge ${b === 'main' ? 'main' : ''}">${b}</span>
              `)}
            </span>
            ${isBaseline && html`<span class="baseline-badge">baseline</span>`}
          </div>
          <div class="commit-subject">${commit.subject || '(no message)'}</div>
          <div class="commit-meta">
            ${formatRelativeTime(commit.timestamp_unix)}
            ${hasHighlights && !expanded && html`
              <span class="highlights-preview">
                ${regressions.length > 0 && html`
                  <span class="hl-badge hl-regression">▼ ${regressions.length} slower</span>
                `}
                ${improvements.length > 0 && html`
                  <span class="hl-badge hl-improvement">▲ ${improvements.length} faster</span>
                `}
              </span>
            `}
          </div>
        </div>
        <div class="commit-result">
          ${ratio > 0 ? (() => {
            const speedup = formatSpeedupVsBaseline(ratio);
            return html`
              <span class="result-value" style=${speedup.color ? `color: ${speedup.color}` : ''}>${speedup.text}</span>
              <span class="result-label">${speedup.label}</span>
            `;
          })() : html`<span class="result-na">—</span>`}
          ${deltaInfo && !isBaseline && html`
            <span class="result-delta" style="color: ${deltaInfo.color}">
              ${deltaInfo.icon} ${deltaInfo.text}
            </span>
          `}
        </div>
      </div>

      <div class="commit-actions">
        ${runUrl && html`<${Link} href=${runUrl} class="action-link">view report<//>`}
        ${hasHighlights && html`
          <button class="expand-btn" onClick=${() => setExpanded(!expanded)}>
            ${expanded ? '▲ less' : '▼ details'}
          </button>
        `}
      </div>

      ${expanded && hasHighlights && html`
        <div class="commit-expansion">
          ${regressions.length > 0 && html`
            <div class="highlights-section">
              <div class="hl-section-title hl-regression-title">Regressions vs baseline</div>
              <div class="hl-list">
                ${regressions.slice(0, 5).map(r => html`
                  <div key=${r.benchmark} class="hl-item hl-regression">
                    <span class="hl-bench">${r.benchmark}</span>
                    <span class="hl-delta">+${Math.abs(r.delta_percent).toFixed(1)}%</span>
                  </div>
                `)}
              </div>
            </div>
          `}
          ${improvements.length > 0 && html`
            <div class="highlights-section">
              <div class="hl-section-title hl-improvement-title">Improvements vs baseline</div>
              <div class="hl-list">
                ${improvements.slice(0, 5).map(i => html`
                  <div key=${i.benchmark} class="hl-item hl-improvement">
                    <span class="hl-bench">${i.benchmark}</span>
                    <span class="hl-delta">${i.delta_percent.toFixed(1)}%</span>
                  </div>
                `)}
              </div>
            </div>
          `}
        </div>
      `}
    </div>
  `;
}

// ============================================================================
// Report Page Components
// ============================================================================

function ReportPage({ branch, commit, operation, selectedCase }) {
  const [runData, setRunData] = useState(null);
  const [indexData, setIndexData] = useState(null);
  const [compareData, setCompareData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [selectedMetric, setSelectedMetric] = useState('instructions');
  const [compareMode, setCompareMode] = useState('none'); // 'none' | 'baseline' | 'parent'
  const [selectedFormat, setSelectedFormat] = useState(null); // null = auto-detect first format
  const [, navigate] = useHashLocation();

  const op = operation || 'deserialize';
  const runUrl = `/runs/${branch}/${commit}/run.json`;

  // Load main run data and index
  useEffect(() => {
    setLoading(true);
    setCompareData(null);
    Promise.all([fetchRunData(runUrl), fetchIndexData()]).then(([run, index]) => {
      if (run) {
        setRunData(run);
      } else {
        setError('Failed to load benchmark data');
      }
      setIndexData(index);
      setLoading(false);
    });
  }, [runUrl]);

  // Load comparison data when compareMode changes
  useEffect(() => {
    if (!indexData || compareMode === 'none') {
      setCompareData(null);
      return;
    }

    let compareUrl = null;

    if (compareMode === 'baseline') {
      const baseline = indexData.baseline;
      if (baseline && baseline.commit_sha !== commit) {
        compareUrl = `/runs/${baseline.branch_key}/${baseline.commit_sha}/run.json`;
      }
    } else if (compareMode === 'parent') {
      // Find parent in branch_commits
      const branchCommits = indexData.branch_commits?.[branch] || [];
      const currentIdx = branchCommits.findIndex(c => c.sha === commit);
      if (currentIdx >= 0 && currentIdx < branchCommits.length - 1) {
        const parent = branchCommits[currentIdx + 1]; // commits are newest-first
        if (parent) {
          compareUrl = `/runs/${branch}/${parent.sha}/run.json`;
        }
      }
    }

    if (compareUrl) {
      fetchRunData(compareUrl).then(data => setCompareData(data));
    } else {
      setCompareData(null);
    }
  }, [compareMode, indexData, branch, commit]);

  if (loading) return html`<div class="loading">Loading report...</div>`;
  if (error) return html`<div class="error">${error}</div>`;
  if (!runData) return html`<div class="error">No data</div>`;

  // Use new catalog structure if available
  const catalog = runData.catalog;
  const isNewSchema = runData.schema === 'run-v1' && catalog;

  // Format handling - get available formats from catalog
  const formats = useMemo(() => {
    if (!isNewSchema || !catalog?.formats_order) return [];
    return catalog.formats_order.map(key => ({
      key,
      ...catalog.formats?.[key]
    })).filter(f => f.label);
  }, [isNewSchema, catalog]);

  // Auto-select first format if none selected
  const activeFormat = selectedFormat || formats[0]?.key || null;

  // Get format config for the active format
  const activeFormatConfig = activeFormat ? catalog?.formats?.[activeFormat] : null;

  // Build metrics list from catalog or fall back to old schema
  const metrics = isNewSchema
    ? Object.entries(catalog.metrics || {}).map(([id, m]) => ({ id, label: m.label, unit: m.unit, better: m.better }))
    : (runData.schema?.metrics || []);

  // Build targets list from catalog or fall back, filtered by format
  const targets = useMemo(() => {
    if (isNewSchema) {
      // Get all targets from catalog
      const allTargets = Object.entries(catalog.targets || {}).map(([id, t]) => ({ id, label: t.label, kind: t.kind }));

      // If no format selected, return all targets
      if (!activeFormat) return allTargets;

      // Get targets for this format from the format's benchmarks
      // Each benchmark has a targets_order that lists applicable targets
      const formatBenchmarks = Object.values(catalog.benchmarks || {}).filter(b => b.format === activeFormat);
      if (formatBenchmarks.length === 0) return allTargets;

      // Collect unique target IDs from format's benchmarks
      const formatTargetIds = new Set();
      formatBenchmarks.forEach(b => {
        (b.targets_order || []).forEach(t => formatTargetIds.add(t));
      });

      // Filter and order targets
      return allTargets.filter(t => formatTargetIds.has(t.id));
    } else {
      return runData.ordering?.targets
        ? runData.ordering.targets.map(id => runData.schema?.targets?.find(t => t.id === id) || { id, label: id })
        : runData.schema?.targets || [];
    }
  }, [isNewSchema, catalog, activeFormat, runData]);

  // Build groups from catalog or fall back, filtered by selected format
  const groups = useMemo(() => {
    if (isNewSchema) {
      return (catalog.groups_order || []).map(groupId => {
        const group = catalog.groups?.[groupId] || {};
        // Filter benchmarks by format
        const filteredBenchmarks = (group.benchmarks_order || []).filter(benchId => {
          if (!activeFormat) return true;
          const benchDef = catalog.benchmarks?.[benchId];
          return benchDef?.format === activeFormat;
        });
        return {
          group_id: groupId,
          label: group.label || sectionLabel(groupId),
          cases: filteredBenchmarks.map(name => {
            const benchDef = catalog.benchmarks?.[name];
            // Display label without format prefix for cleaner UI
            const displayLabel = benchDef?.label || name;
            return { case_id: name, label: displayLabel };
          })
        };
      }).filter(g => g.cases.length > 0); // Hide empty groups
    } else {
      return runData.ordering?.sections
        ? runData.ordering.sections.map(section => ({
            group_id: section,
            label: sectionLabel(section),
            cases: (runData.ordering.benchmarks?.[section] || []).map(name => ({ case_id: name, label: name }))
          }))
        : runData.groups || [];
    }
  }, [isNewSchema, catalog, runData, activeFormat]);

  const branchItems = indexData?.branches ?
    Object.keys(indexData.branches).map(b => ({ value: b, label: b })) : [];
  const commitItems = indexData?.branch_commits?.[branch]?.map(c => ({
    value: c.sha,
    label: c.short,
    meta: formatRelativeTime(c.timestamp_unix)
  })) || [];

  // Build comparison options
  const compareItems = [
    { value: 'none', label: 'No comparison' },
    { value: 'baseline', label: `vs baseline (${indexData?.baseline?.commit_short || 'main'})` },
    { value: 'parent', label: 'vs previous commit' }
  ];

  const compareModeLabel = compareItems.find(i => i.value === compareMode)?.label || 'Compare';

  // Helper to navigate to a different benchmark/overview
  const navigateToCase = (caseId) => {
    navigate(`/runs/${branch}/${commit}/${op}/${caseId}`);
  };

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
              if (firstCommit) navigate(`/runs/${b}/${firstCommit}/${op}/overview`);
            }}
          />
          <span class="nav-sep">/</span>
          <${Dropdown}
            trigger=${commit.slice(0, 8)}
            items=${commitItems}
            value=${commit}
            onChange=${(c) => navigate(`/runs/${branch}/${c}/${op}/overview`)}
          />
        </div>
        <div class="nav-right">
          ${formats.length > 1 && html`
            <${Dropdown}
              trigger=${activeFormatConfig?.label || activeFormat || 'Format'}
              items=${formats.map(f => ({ value: f.key, label: f.label }))}
              value=${activeFormat}
              onChange=${setSelectedFormat}
            />
          `}
          <${Dropdown}
            trigger=${compareModeLabel}
            items=${compareItems}
            value=${compareMode}
            onChange=${setCompareMode}
          />
          <div class="op-toggle">
            <button
              class=${op === 'deserialize' ? 'active' : ''}
              onClick=${() => navigate(`/runs/${branch}/${commit}/deserialize/${selectedCase}`)}
            >deser</button>
            <button
              class=${op === 'serialize' ? 'active' : ''}
              onClick=${() => navigate(`/runs/${branch}/${commit}/serialize/${selectedCase}`)}
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
          <button
            class="sidebar-case ${selectedCase === 'overview' ? 'active' : ''}"
            onClick=${() => navigateToCase('overview')}
          >
            Overview
          </button>
          <div class="sidebar-divider"></div>
          ${groups.map(group => html`
            <div key=${group.group_id} class="sidebar-group">
              <div class="group-label">${group.label}</div>
              ${group.cases.map(c => html`
                <button
                  key=${c.case_id}
                  class="sidebar-case ${selectedCase === c.case_id ? 'active' : ''}"
                  onClick=${() => navigateToCase(c.case_id)}
                >
                  ${c.label}
                </button>
              `)}
            </div>
          `)}
        </aside>

        <main class="report-main">
          ${selectedCase === 'overview' ? html`
            <${OverviewView}
              runData=${runData}
              metrics=${metrics}
              selectedMetric=${selectedMetric}
              operation=${op}
              isNewSchema=${isNewSchema}
              onSelectBenchmark=${navigateToCase}
              activeFormat=${activeFormat}
              activeFormatConfig=${activeFormatConfig}
            />
          ` : selectedCase && html`
            <${CaseView}
              caseId=${selectedCase}
              caseData=${isNewSchema ? runData.results?.values?.[selectedCase] : runData.results?.[selectedCase]}
              compareData=${isNewSchema ? compareData?.results?.values?.[selectedCase] : compareData?.results?.[selectedCase]}
              targets=${targets}
              metrics=${metrics}
              selectedMetric=${selectedMetric}
              operation=${op}
              compareMode=${compareMode}
              isNewSchema=${isNewSchema}
              activeFormatConfig=${activeFormatConfig}
              catalog=${catalog}
            />
          `}
        </main>
      </div>
    </div>
  `;
}

function sectionLabel(section) {
  const labels = {
    micro: 'Micro Benchmarks',
    synthetic: 'Synthetic Benchmarks',
    realistic: 'Realistic Benchmarks',
    other: 'Other'
  };
  return labels[section] || section;
}

function CaseView({ caseId, caseData, compareData, targets, metrics, selectedMetric, operation, compareMode, isNewSchema, activeFormatConfig, catalog }) {
  if (!caseData) return html`<div class="no-data">No data for ${caseId}</div>`;

  const metricInfo = metrics.find(m => m.id === selectedMetric);

  // Get the format for this benchmark to determine baseline target
  const benchFormat = catalog?.benchmarks?.[caseId]?.format;
  const formatConfig = benchFormat ? catalog?.formats?.[benchFormat] : activeFormatConfig;
  const baselineTargetId = formatConfig?.baseline_target || 'serde_json';
  const baselineTargetLabel = baselineTargetId;

  // Helper to get metric value from either schema
  const getMetricValue = (data, targetId, metricId) => {
    if (!data) return null;
    if (isNewSchema) {
      // New schema: caseData is results.values[benchmark]
      // Structure: { operation: { target: { metric: value } } }
      return data?.[operation]?.[targetId]?.[metricId] ?? null;
    } else {
      // Old schema: caseData.targets[targetId].ops[operation].metrics[metricId]
      const result = data?.targets?.[targetId]?.ops?.[operation];
      return result?.ok ? result?.metrics?.[metricId] : null;
    }
  };

  const baselineValue = getMetricValue(caseData, baselineTargetId, selectedMetric);

  // Compute chart data (include all targets, even missing ones)
  const chartData = targets
    .map(target => {
      const value = getMetricValue(caseData, target.id, selectedMetric);
      const compareValue = getMetricValue(compareData, target.id, selectedMetric);
      return { target, value, compareValue, isMissing: value === null };
    });

  const maxValue = Math.max(...chartData.filter(d => !d.isMissing).map(d => Math.max(d.value || 0, d.compareValue || 0)));

  return html`
    <div class="case-view">
      <h2 class="case-title">${caseId}</h2>

      <${BarChart}
        data=${chartData}
        maxValue=${maxValue}
        baselineValue=${baselineValue}
        metricInfo=${metricInfo}
        selectedMetric=${selectedMetric}
        compareMode=${compareMode}
      />

      <table class="results-table">
        <thead>
          <tr>
            <th>Target</th>
            <th>${metricInfo?.label || selectedMetric}</th>
            <th>vs ${baselineTargetLabel}</th>
            ${compareMode !== 'none' && html`<th>Δ vs ${compareMode}</th>`}
          </tr>
        </thead>
        <tbody>
          ${targets.map(target => {
            const value = getMetricValue(caseData, target.id, selectedMetric);
            const isMissing = value === null;

            const ratio = value && baselineValue ? value / baselineValue : null;
            const ratioInfo = formatRatioVsBaseline(ratio);

            // Comparison delta
            const compareValue = getMetricValue(compareData, target.id, selectedMetric);
            const compareDelta = value && compareValue ? ((value - compareValue) / compareValue) * 100 : null;
            const compareDeltaInfo = compareDelta !== null ? formatDelta(compareDelta) : null;

            // Get tier indicator for JIT targets
            const targetData = isNewSchema ? caseData?.[operation]?.[target.id] : caseData?.targets?.[target.id]?.ops?.[operation]?.metrics;
            const tierIndicator = getTierIndicator(targetData, target.id);

            return html`
              <tr key=${target.id} class="${target.kind === 'baseline' ? 'baseline-row' : ''} ${isMissing ? 'missing-row' : ''}">
                <td class="target-cell">
                  <span class="target-label">${target.label}</span>
                  ${target.kind === 'baseline' && html`<span class="baseline-tag">baseline</span>`}
                  ${tierIndicator && html`<span class="tier-indicator" style="color: ${tierIndicator.color}" title="${tierIndicator.title}">${tierIndicator.icon} ${tierIndicator.label}</span>`}
                </td>
                <td class="value-cell">
                  ${isMissing ? html`<span class="missing-value">(missing)</span>` : formatMetricValue(value, selectedMetric)}
                </td>
                <td class="ratio-cell">
                  ${isMissing ? html`<span class="missing-value">—</span>` : html`<span style=${ratioInfo.color ? `color: ${ratioInfo.color}` : ''}>${ratioInfo.text}</span>`}
                </td>
                ${compareMode !== 'none' && html`
                  <td class="delta-cell">
                    ${isMissing ? html`<span class="missing-value">—</span>` : compareDeltaInfo ? html`
                      <span style="color: ${compareDeltaInfo.color}">
                        ${compareDeltaInfo.icon} ${compareDeltaInfo.text}
                      </span>
                    ` : '—'}
                  </td>
                `}
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
        isNewSchema=${isNewSchema}
      />
    </div>
  `;
}

// ============================================================================
// Bar Chart Component
// ============================================================================

function BarChart({ data, maxValue, baselineValue, metricInfo, selectedMetric, compareMode }) {
  if (!data || data.length === 0) return null;

  const barHeight = 28;
  const labelWidth = 140;
  const chartWidth = 500;
  const gap = 8;
  const height = data.length * (barHeight + gap) + 20;

  return html`
    <div class="chart-container">
      <svg class="bar-chart" viewBox="0 0 ${labelWidth + chartWidth + 140} ${height}" preserveAspectRatio="xMinYMin meet">
        ${data.map((d, i) => {
          const y = i * (barHeight + gap) + 10;
          const isSerde = d.target.kind === 'baseline';

          // Handle missing data
          if (d.isMissing) {
            return html`
              <g key=${d.target.id} class="chart-missing">
                <text
                  x=${labelWidth - 8}
                  y=${y + barHeight / 2 + 4}
                  text-anchor="end"
                  class="chart-label chart-label-missing"
                >${d.target.label}</text>
                <text
                  x=${labelWidth + 6}
                  y=${y + barHeight / 2 + 4}
                  class="chart-value-missing"
                >(missing)</text>
              </g>
            `;
          }

          const barWidth = maxValue > 0 ? (d.value / maxValue) * chartWidth : 0;
          const compareWidth = maxValue > 0 && d.compareValue ? (d.compareValue / maxValue) * chartWidth : 0;

          // Color based on whether this is baseline or facet
          const barColor = isSerde ? 'var(--chart-baseline)' : 'var(--chart-facet)';

          // Compute ratio vs baseline
          const ratio = baselineValue && d.value ? d.value / baselineValue : null;
          const ratioInfo = formatRatioVsBaseline(ratio);

          return html`
            <g key=${d.target.id}>
              <!-- Label -->
              <text
                x=${labelWidth - 8}
                y=${y + barHeight / 2 + 4}
                text-anchor="end"
                class="chart-label"
              >${d.target.label}</text>

              <!-- Comparison bar (if present) -->
              ${compareMode !== 'none' && compareWidth > 0 && html`
                <rect
                  x=${labelWidth}
                  y=${y + 2}
                  width=${compareWidth}
                  height=${barHeight - 4}
                  fill="var(--chart-compare)"
                  rx="2"
                />
              `}

              <!-- Main bar -->
              <rect
                x=${labelWidth}
                y=${y + (compareMode !== 'none' ? 6 : 2)}
                width=${barWidth}
                height=${compareMode !== 'none' ? barHeight - 12 : barHeight - 4}
                fill=${barColor}
                rx="2"
              />

              <!-- Value label with ratio -->
              <text
                x=${labelWidth + barWidth + 6}
                y=${y + barHeight / 2 + 4}
                class="chart-value"
              >${formatMetricValue(d.value, selectedMetric)}${!isSerde && ratio ? html` <tspan fill=${ratioInfo.color}>(${ratioInfo.text})</tspan>` : ''}</text>
            </g>
          `;
        })}
      </svg>
      ${compareMode !== 'none' && html`
        <div class="chart-legend">
          <span class="legend-item"><span class="legend-color" style="background: var(--chart-facet)"></span>Current</span>
          <span class="legend-item"><span class="legend-color" style="background: var(--chart-compare)"></span>${compareMode === 'baseline' ? 'Baseline' : 'Previous'}</span>
        </div>
      `}
    </div>
  `;
}

function MetricsDetail({ caseData, targets, metrics, operation, isNewSchema }) {
  // Helper to check if target has data
  const hasData = (targetId) => {
    if (isNewSchema) {
      return caseData?.[operation]?.[targetId] != null;
    } else {
      return caseData?.targets?.[targetId]?.ops?.[operation]?.ok;
    }
  };

  // Helper to get metric value
  const getMetric = (targetId, metricId) => {
    if (isNewSchema) {
      return caseData?.[operation]?.[targetId]?.[metricId] ?? null;
    } else {
      return caseData?.targets?.[targetId]?.ops?.[operation]?.metrics?.[metricId] ?? null;
    }
  };

  return html`
    <details class="metrics-detail">
      <summary>All metrics</summary>
      <div class="metrics-grid">
        ${targets.filter(t => hasData(t.id)).map(target => html`
          <div key=${target.id} class="metrics-card">
            <div class="metrics-card-header">${target.label}</div>
            <div class="metrics-card-body">
              ${metrics.map(m => {
                const val = getMetric(target.id, m.id);
                return val !== undefined && val !== null ? html`
                  <div key=${m.id} class="metric-row">
                    <span class="metric-label">${m.label}</span>
                    <span class="metric-value">${formatMetricValue(val, m.id)}</span>
                  </div>
                ` : null;
              })}
            </div>
          </div>
        `)}
      </div>
    </details>
  `;
}

// ============================================================================
// Overview Components
// ============================================================================

function OverviewSummary({ stats, baselineLabel = 'baseline' }) {
  const avgRatioInfo = formatRatioVsBaseline(stats.avgRatio);

  return html`
    <div class="overview-summary">
      <div class="overview-stat">
        <div class="overview-stat-label">Total Benchmarks</div>
        <div class="overview-stat-value">${stats.total}</div>
      </div>
      <div class="overview-stat">
        <div class="overview-stat-label">Faster</div>
        <div class="overview-stat-value" style="color: var(--good)">
          ${stats.faster}
        </div>
      </div>
      <div class="overview-stat">
        <div class="overview-stat-label">Slower</div>
        <div class="overview-stat-value" style="color: var(--bad)">
          ${stats.slower}
        </div>
      </div>
      <div class="overview-stat">
        <div class="overview-stat-label">Neutral</div>
        <div class="overview-stat-value" style="color: var(--muted)">
          ${stats.neutral}
        </div>
      </div>
      <div class="overview-stat">
        <div class="overview-stat-label">Avg vs ${baselineLabel}</div>
        <div class="overview-stat-value" style="color: ${avgRatioInfo.color}">
          ${avgRatioInfo.text}
        </div>
      </div>
    </div>
  `;
}

// Grouped bars chart - side-by-side comparison
function GroupedBarsChart({ data, metricDef, onSelectBenchmark }) {
  const barHeight = 24;
  const labelWidth = 180;
  const chartWidth = 600;
  const gap = 4;
  const height = data.length * (barHeight + gap) + 20;

  const maxValue = Math.max(...data.map(d => Math.max(d.baselineValue, d.facetValue)));

  return html`
    <svg class="overview-chart" viewBox="0 0 ${labelWidth + chartWidth + 100} ${height}">
      ${data.map((d, i) => {
        const y = i * (barHeight + gap) + 10;
        const baselineWidth = (d.baselineValue / maxValue) * chartWidth;
        const facetWidth = (d.facetValue / maxValue) * chartWidth;
        const barH = (barHeight - gap) / 2;

        return html`
          <g key=${d.id} class="chart-row" onClick=${() => onSelectBenchmark(d.id)} style="cursor: pointer">
            <text x=${labelWidth - 8} y=${y + barHeight / 2 + 4} text-anchor="end" class="chart-label">
              ${d.name}
            </text>
            <!-- baseline bar -->
            <rect x=${labelWidth} y=${y} width=${baselineWidth} height=${barH} fill="var(--chart-baseline)" rx="2" />
            <!-- facet bar -->
            <rect x=${labelWidth} y=${y + barH + 2} width=${facetWidth} height=${barH}
              fill=${d.ratio < 1 ? 'var(--good)' : 'var(--bad)'} rx="2" />
          </g>
        `;
      })}
    </svg>
  `;
}

// Diverging bars chart - ratio from center
function DivergingBarsChart({ data, onSelectBenchmark }) {
  const barHeight = 24;
  const labelWidth = 180;
  const chartWidth = 600;
  const gap = 4;
  const centerX = labelWidth + chartWidth / 2;
  const height = data.length * (barHeight + gap) + 20;

  // Find max deviation from 1.0 for scaling
  const maxDeviation = Math.max(...data.map(d => Math.abs(d.ratio - 1)));
  const scale = (chartWidth / 2) / (maxDeviation + 0.1);

  return html`
    <svg class="overview-chart" viewBox="0 0 ${labelWidth + chartWidth + 100} ${height}">
      <!-- Center baseline -->
      <line x1=${centerX} y1="0" x2=${centerX} y2=${height}
        stroke="var(--border)" stroke-width="2" stroke-dasharray="4 4" />

      ${data.map((d, i) => {
        const y = i * (barHeight + gap) + 10;
        const deviation = d.ratio - 1;
        const barWidth = Math.abs(deviation) * scale;
        const x = deviation < 0 ? centerX - barWidth : centerX;
        const color = deviation < 0 ? 'var(--good)' : 'var(--bad)';

        return html`
          <g key=${d.id} class="chart-row" onClick=${() => onSelectBenchmark(d.id)} style="cursor: pointer">
            <text x=${labelWidth - 8} y=${y + barHeight / 2 + 4} text-anchor="end" class="chart-label">
              ${d.name}
            </text>
            <rect x=${x} y=${y + 2} width=${barWidth} height=${barHeight - 4} fill=${color} rx="2" />
            <text x=${deviation < 0 ? x - 4 : x + barWidth + 4} y=${y + barHeight / 2 + 4}
              text-anchor=${deviation < 0 ? 'end' : 'start'} class="chart-value" fill=${color}>
              ${d.ratio.toFixed(2)}×
            </text>
          </g>
        `;
      })}
    </svg>
  `;
}

// Dot plot chart - scatter with baseline
function DotPlotChart({ data, metricDef, onSelectBenchmark }) {
  const barHeight = 24;
  const labelWidth = 180;
  const chartWidth = 600;
  const gap = 4;
  const height = data.length * (barHeight + gap) + 20;

  const maxValue = Math.max(...data.map(d => Math.max(d.baselineValue, d.facetValue)));

  return html`
    <svg class="overview-chart" viewBox="0 0 ${labelWidth + chartWidth + 100} ${height}">
      ${data.map((d, i) => {
        const y = i * (barHeight + gap) + 10 + barHeight / 2;
        const baselineX = labelWidth + (d.baselineValue / maxValue) * chartWidth;
        const facetX = labelWidth + (d.facetValue / maxValue) * chartWidth;

        return html`
          <g key=${d.id} class="chart-row" onClick=${() => onSelectBenchmark(d.id)} style="cursor: pointer">
            <text x=${labelWidth - 8} y=${y + 4} text-anchor="end" class="chart-label">
              ${d.name}
            </text>
            <!-- Connecting line -->
            <line x1=${baselineX} y1=${y} x2=${facetX} y2=${y}
              stroke=${d.ratio < 1 ? 'var(--good)' : 'var(--bad)'}
              stroke-width="2" opacity="0.3" />
            <!-- baseline dot -->
            <circle cx=${baselineX} cy=${y} r="4" fill="var(--chart-baseline)" />
            <!-- facet dot -->
            <circle cx=${facetX} cy=${y} r="5"
              fill=${d.ratio < 1 ? 'var(--good)' : 'var(--bad)'}
              stroke="white" stroke-width="1.5" />
          </g>
        `;
      })}
    </svg>
  `;
}

function OverviewTable({ data, sortBy, sortDir, onSort, onSelectBenchmark, metricDef, baselineLabel, primaryLabel }) {
  const sortIndicator = (col) => {
    if (sortBy !== col) return '';
    return sortDir === 'asc' ? ' ▲' : ' ▼';
  };

  return html`
    <table class="overview-table">
      <thead>
        <tr>
          <th class="sortable" onClick=${() => onSort('name')}>
            Benchmark${sortIndicator('name')}
          </th>
          <th class="sortable" onClick=${() => onSort('group')}>
            Group${sortIndicator('group')}
          </th>
          <th class="sortable numeric" onClick=${() => onSort('baseline')}>
            ${baselineLabel}${sortIndicator('baseline')}
          </th>
          <th class="sortable numeric" onClick=${() => onSort('facet')}>
            ${primaryLabel}${sortIndicator('facet')}
          </th>
          <th class="sortable numeric" onClick=${() => onSort('ratio')}>
            Ratio${sortIndicator('ratio')}
          </th>
        </tr>
      </thead>
      <tbody>
        ${data.map(d => {
          const ratioInfo = formatRatioVsBaseline(d.ratio);
          return html`
            <tr class="overview-row" key=${d.id}>
              <td class="bench-name-cell">
                <a href="#" onClick=${(e) => { e.preventDefault(); onSelectBenchmark(d.id); }}>
                  ${d.name}
                </a>
              </td>
              <td class="group-cell">
                <span class="group-badge group-${d.group}">
                  ${d.group}
                </span>
              </td>
              <td class="value-cell numeric">
                ${formatMetricValue(d.baselineValue, metricDef?.id)}
              </td>
              <td class="value-cell numeric">
                ${formatMetricValue(d.facetValue, metricDef?.id)}
              </td>
              <td class="ratio-cell numeric">
                <span class="ratio-value" style="color: ${ratioInfo.color}">
                  ${ratioInfo.text}
                </span>
                ${d.ratio < 1 ? html`
                  <span class="ratio-badge faster">faster</span>
                ` : d.ratio > 1.02 ? html`
                  <span class="ratio-badge slower">slower</span>
                ` : ''}
              </td>
            </tr>
          `;
        })}
      </tbody>
    </table>
  `;
}

function OverviewView({ runData, metrics, selectedMetric, operation, isNewSchema, onSelectBenchmark, activeFormat, activeFormatConfig }) {
  const [sortBy, setSortBy] = useState('ratio');
  const [sortDir, setSortDir] = useState('asc'); // asc = best speedups first
  const [vizMode, setVizMode] = useState('grouped'); // 'grouped' | 'diverging' | 'dots'

  // Extract benchmarks - handle both old and new schema, filtered by format
  const benchmarks = useMemo(() => {
    if (isNewSchema) {
      const allBenchmarks = Object.keys(runData.results?.values || {});
      if (!activeFormat) return allBenchmarks;
      // Filter by format
      return allBenchmarks.filter(benchId => {
        const benchDef = runData.catalog?.benchmarks?.[benchId];
        return benchDef?.format === activeFormat;
      });
    } else {
      return Object.keys(runData.results || {});
    }
  }, [isNewSchema, runData, activeFormat]);

  // Get baseline and primary targets for current format
  const baselineTarget = activeFormatConfig?.baseline_target || 'serde_json';
  const primaryTarget = activeFormatConfig?.primary_target || 'facet_json_t2';

  const overviewData = useMemo(() => {
    const data = benchmarks.map(benchId => {
      let baselineValue, facetValue;

      if (isNewSchema) {
        // New schema: results.values[benchmark][operation][target][metric]
        const benchData = runData.results.values[benchId];
        baselineValue = benchData?.[operation]?.[baselineTarget]?.[selectedMetric];
        // Use primary target from format config, with fallbacks for JIT tiers
        facetValue = benchData?.[operation]?.[primaryTarget]?.[selectedMetric];
        // Fallback to lower tiers if primary not available (for JSON with t2/t1/t0)
        if (facetValue === undefined && primaryTarget.includes('_t2')) {
          const baseTarget = primaryTarget.replace('_t2', '');
          facetValue = benchData?.[operation]?.[`${baseTarget}_t1`]?.[selectedMetric]
            || benchData?.[operation]?.[`${baseTarget}_t0`]?.[selectedMetric];
        }
      } else {
        // Old schema: results[benchmark].targets[target].ops[operation].metrics[metric]
        const benchData = runData.results[benchId];
        const baselineResult = benchData?.targets?.[baselineTarget]?.ops?.[operation];
        // Try primary target first, then fallback tiers if it's a JIT target
        let facetResult = benchData?.targets?.[primaryTarget]?.ops?.[operation];
        if (!facetResult && primaryTarget.includes('_t2')) {
          const baseTarget = primaryTarget.replace('_t2', '');
          facetResult = benchData?.targets?.[`${baseTarget}_t1`]?.ops?.[operation]
            || benchData?.targets?.[`${baseTarget}_t0`]?.ops?.[operation];
        }
        baselineValue = baselineResult?.ok ? baselineResult?.metrics?.[selectedMetric] : null;
        facetValue = facetResult?.ok ? facetResult?.metrics?.[selectedMetric] : null;
      }

      const ratio = baselineValue && facetValue ? facetValue / baselineValue : null;
      const group = isNewSchema
        ? findBenchmarkGroup(benchId, runData.catalog)
        : findBenchmarkGroup(benchId, { groups: runData.groups?.reduce((acc, g) => {
            acc[g.group_id] = { benchmarks_order: g.cases?.map(c => c.case_id) };
            return acc;
          }, {}) });

      // Get display name (label without format prefix)
      const benchDef = runData.catalog?.benchmarks?.[benchId];
      const displayName = benchDef?.label || benchId;

      return {
        id: benchId,
        name: displayName,
        group,
        baselineValue,
        facetValue,
        ratio,
        hasBothValues: baselineValue !== null && facetValue !== null
      };
    }).filter(d => d.hasBothValues); // Only show benchmarks with both values

    return data;
  }, [runData, selectedMetric, operation, benchmarks, isNewSchema, baselineTarget, primaryTarget]);

  // Sort data
  const sortedData = useMemo(() => {
    const sorted = [...overviewData];
    sorted.sort((a, b) => {
      let aVal, bVal;
      switch(sortBy) {
        case 'name': aVal = a.name; bVal = b.name; break;
        case 'group': aVal = a.group; bVal = b.group; break;
        case 'baseline': aVal = a.baselineValue || 0; bVal = b.baselineValue || 0; break;
        case 'facet': aVal = a.facetValue || 0; bVal = b.facetValue || 0; break;
        case 'ratio': aVal = a.ratio || 0; bVal = b.ratio || 0; break;
        default: return 0;
      }
      const comparison = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
      return sortDir === 'asc' ? comparison : -comparison;
    });
    return sorted;
  }, [overviewData, sortBy, sortDir]);

  // Compute summary stats
  const stats = useMemo(() => {
    if (overviewData.length === 0) {
      return { total: 0, faster: 0, slower: 0, neutral: 0, avgRatio: 1 };
    }
    return {
      total: overviewData.length,
      faster: overviewData.filter(d => d.ratio < 1).length,
      slower: overviewData.filter(d => d.ratio > 1.02).length,
      neutral: overviewData.filter(d => d.ratio >= 1 && d.ratio <= 1.02).length,
      avgRatio: overviewData.reduce((sum, d) => sum + d.ratio, 0) / overviewData.length
    };
  }, [overviewData]);

  const handleSort = (column) => {
    if (sortBy === column) {
      setSortDir(sortDir === 'asc' ? 'desc' : 'asc');
    } else {
      setSortBy(column);
      setSortDir('asc');
    }
  };

  const metricDef = metrics.find(m => m.id === selectedMetric);

  // Get labels for baseline/primary targets
  const baselineLabel = activeFormatConfig?.baseline_target || 'baseline';
  const primaryLabel = activeFormatConfig?.primary_target || 'facet';

  if (overviewData.length === 0) {
    const totalBenchmarks = benchmarks.length;
    return html`
      <div class="no-data">
        <p>No benchmark data available for comparison.</p>
        <p style="color: var(--muted); font-size: 13px; margin-top: 0.5rem;">
          Found ${totalBenchmarks} benchmark(s), but none have both ${baselineLabel} and ${primaryLabel} results for ${operation}.
        </p>
      </div>
    `;
  }

  return html`
    <div class="overview-view">
      <h2 class="case-title">Overview: All Benchmarks</h2>

      <${OverviewSummary} stats=${stats} baselineLabel=${baselineLabel} />

      <div class="viz-mode-selector">
        <button
          class="viz-mode-btn ${vizMode === 'grouped' ? 'active' : ''}"
          onClick=${() => setVizMode('grouped')}
        >Grouped Bars</button>
        <button
          class="viz-mode-btn ${vizMode === 'diverging' ? 'active' : ''}"
          onClick=${() => setVizMode('diverging')}
        >Diverging Bars</button>
        <button
          class="viz-mode-btn ${vizMode === 'dots' ? 'active' : ''}"
          onClick=${() => setVizMode('dots')}
        >Dot Plot</button>
      </div>

      <div class="overview-chart-container">
        ${vizMode === 'grouped' && html`
          <${GroupedBarsChart}
            data=${sortedData}
            metricDef=${metricDef}
            onSelectBenchmark=${onSelectBenchmark}
          />
        `}
        ${vizMode === 'diverging' && html`
          <${DivergingBarsChart}
            data=${sortedData}
            onSelectBenchmark=${onSelectBenchmark}
          />
        `}
        ${vizMode === 'dots' && html`
          <${DotPlotChart}
            data=${sortedData}
            metricDef=${metricDef}
            onSelectBenchmark=${onSelectBenchmark}
          />
        `}
      </div>

      <${OverviewTable}
        data=${sortedData}
        sortBy=${sortBy}
        sortDir=${sortDir}
        onSort=${handleSort}
        onSelectBenchmark=${onSelectBenchmark}
        metricDef=${metricDef}
        baselineLabel=${baselineLabel}
        primaryLabel=${primaryLabel}
      />
    </div>
  `;
}

// ============================================================================
// App Router
// ============================================================================

// Wrapper to extract params for ReportPage
function ReportRoute() {
  const params = useParams();
  return html`<${ReportPage}
    branch=${params.branch}
    commit=${params.commit}
    operation=${params.operation || 'deserialize'}
    selectedCase=${params.case || 'overview'}
  />`;
}

function NotFound() {
  return html`
    <div class="not-found">
      <h1>404</h1>
      <p>Page not found</p>
      <${Link} href="/">← Back to index<//>
    </div>
  `;
}

function App() {
  return html`
    <${HashRouter}>
      <${Route} path="/" component=${IndexPage} />
      <${Route} path="/runs/:branch/:commit/:operation?/:case?" component=${ReportRoute} />
      <${Route} path="/:rest*" component=${NotFound} />
    <//>
  `;
}

// ============================================================================
// Styles
// ============================================================================

const styles = `
/* CSS Variables for charts */
:root {
  --chart-baseline: #6b7280;
  --chart-facet: #3b82f6;
  --chart-compare: rgba(156, 163, 175, 0.4);
}

/* Tabular numerals for consistent number alignment */
.result-value, .result-delta,
.value-cell, .ratio-cell, .delta-cell,
.metric-value, .hl-delta,
.chart-value {
  font-variant-numeric: tabular-nums;
}

/* Form element reset - inherit font from body */
button, input, select, textarea {
  font-family: inherit;
}

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

/* Commit Timeline */
.commit-timeline { }

.commit-row {
  border-bottom: 1px solid var(--border);
  padding: 0.75rem 1rem;
  transition: background 0.1s;
}
.commit-row:hover { background: var(--panel2); }
.commit-row.is-baseline { background: var(--panel); border-left: 3px solid var(--accent); }

.commit-main {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 2rem;
}
.commit-info { flex: 1; min-width: 0; }

.commit-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.25rem;
  flex-wrap: wrap;
}
.commit-sha {
  font-family: var(--mono);
  font-weight: 600;
  font-size: 13px;
  color: var(--accent);
}
.commit-branches { display: flex; gap: 0.25rem; flex-wrap: wrap; }
/* Unified badge styling */
.branch-badge,
.baseline-badge,
.baseline-tag,
.hl-badge,
.group-badge,
.ratio-badge,
.tier-indicator {
  display: inline-block;
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 3px;
  font-weight: 600;
  text-transform: uppercase;
}

.branch-badge {
  background: var(--panel2);
  color: var(--muted);
}
.branch-badge.main {
  background: var(--accent);
  color: white;
}
.baseline-badge,
.baseline-tag {
  background: var(--good);
  color: white;
  margin-left: 0.5rem;
}

.commit-subject {
  font-size: 14px;
  margin-bottom: 0.25rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.commit-meta { font-size: 12px; color: var(--muted); }

.commit-result {
  text-align: right;
  white-space: nowrap;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.125rem;
}
.result-value { font-weight: 700; font-size: 18px; }
.result-label { font-size: 11px; color: var(--muted); text-transform: uppercase; }
.result-na { color: var(--muted); }
.result-delta { font-weight: 600; font-size: 13px; }

.commit-actions {
  margin-top: 0.5rem;
  display: flex;
  gap: 1rem;
  font-size: 13px;
}
.action-link {
  color: var(--accent);
  text-decoration: none;
}
.action-link:hover { text-decoration: underline; }
.expand-btn {
  background: none;
  border: none;
  color: var(--muted);
  cursor: pointer;
  font-size: 12px;
  padding: 0;
}
.expand-btn:hover { color: var(--text); }

/* Highlights preview badges */
.highlights-preview {
  margin-left: 0.75rem;
  display: inline-flex;
  gap: 0.5rem;
}
.hl-badge.hl-regression {
  background: rgba(239, 68, 68, 0.15);
  color: var(--bad);
}
.hl-badge.hl-improvement {
  background: rgba(34, 197, 94, 0.15);
  color: var(--good);
}

/* Expansion panel */
.commit-expansion {
  margin-top: 0.75rem;
  padding: 0.75rem;
  background: var(--panel);
  border-radius: 6px;
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 1rem;
}
.highlights-section { }
.hl-section-title {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  margin-bottom: 0.5rem;
}
.hl-regression-title { color: var(--bad); }
.hl-improvement-title { color: var(--good); }
.hl-list { display: flex; flex-direction: column; gap: 0.25rem; }
.hl-item {
  display: flex;
  justify-content: space-between;
  padding: 0.25rem 0.5rem;
  border-radius: 4px;
  font-size: 12px;
}
.hl-item.hl-regression { background: rgba(239, 68, 68, 0.08); }
.hl-item.hl-improvement { background: rgba(34, 197, 94, 0.08); }
.hl-bench { font-family: var(--mono); }
.hl-delta {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.hl-item.hl-regression .hl-delta { color: var(--bad); }
.hl-item.hl-improvement .hl-delta { color: var(--good); }

.commit-row.expanded { background: var(--panel2); }

.no-results {
  padding: 2rem;
  text-align: center;
  color: var(--muted);
}

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

/* Chart */
.chart-container { margin-bottom: 1.5rem; }
.bar-chart {
  width: 100%;
  max-width: 720px;
  height: auto;
}
.chart-label {
  font-family: var(--mono);
  font-size: 12px;
  fill: var(--text);
}
.chart-value {
  font-family: var(--mono);
  font-size: 11px;
  fill: var(--muted);
  font-variant-numeric: tabular-nums;
}
.chart-label-missing {
  fill: var(--muted);
  opacity: 0.6;
}
.chart-value-missing {
  font-family: var(--mono);
  font-size: 11px;
  fill: var(--muted);
  font-style: italic;
  opacity: 0.6;
}
.chart-legend {
  display: flex;
  gap: 1rem;
  margin-top: 0.5rem;
  font-size: 12px;
  color: var(--muted);
}
.legend-item { display: flex; align-items: center; gap: 0.25rem; }
.legend-color { width: 12px; height: 12px; border-radius: 2px; }

/* Results Table */
.results-table { width: 100%; border-collapse: collapse; margin-bottom: 1.5rem; }
.results-table th, .results-table td { padding: 0.5rem 1rem; text-align: left; border-bottom: 1px solid var(--border); }
.results-table th { font-weight: 600; font-size: 12px; text-transform: uppercase; color: var(--muted); }
.baseline-row { background: var(--panel2); }
.target-cell { }
.target-label { font-weight: 500; }
.value-cell { font-variant-numeric: tabular-nums; }
.ratio-cell { font-variant-numeric: tabular-nums; font-weight: 600; }
.delta-cell { font-variant-numeric: tabular-nums; font-weight: 500; }
.error-value { color: var(--bad); font-style: italic; }
.missing-row { opacity: 0.6; }
.missing-value { color: var(--muted); font-style: italic; font-size: 12px; }

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

/* Overview Components */
.sidebar-divider {
  border-top: 1px solid var(--border);
  margin: 0.75rem 0;
}

.viz-mode-selector {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1.5rem;
}

.viz-mode-btn {
  padding: 0.4rem 1rem;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text);
  cursor: pointer;
  font-size: 13px;
  transition: all 0.15s;
}

.viz-mode-btn:hover {
  background: var(--panel2);
}

.viz-mode-btn.active {
  background: var(--accent);
  color: white;
  border-color: var(--accent);
}

.overview-chart-container {
  margin-bottom: 2rem;
  overflow-x: auto;
}

.overview-chart {
  width: 100%;
  max-width: 880px;
  height: auto;
}

.overview-chart .chart-row {
  transition: opacity 0.15s;
}

.overview-chart .chart-row:hover {
  opacity: 0.7;
}

.overview-chart .chart-label {
  font-family: var(--mono);
  font-size: 11px;
  fill: var(--text);
}

.overview-chart .chart-value {
  font-family: var(--mono);
  font-size: 10px;
}

.overview-summary {
  display: flex;
  gap: 2rem;
  padding: 1.25rem;
  background: var(--panel);
  border-radius: 6px;
  margin-bottom: 1.5rem;
  border: 1px solid var(--border);
}
.overview-stat {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.overview-stat-label {
  font-size: 11px;
  color: var(--muted);
  text-transform: uppercase;
  font-weight: 600;
  letter-spacing: 0.5px;
}
.overview-stat-value {
  font-size: 24px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.overview-table {
  width: 100%;
  border-collapse: collapse;
  margin-top: 1rem;
}
.overview-table th {
  padding: 0.75rem 1rem;
  text-align: left;
  border-bottom: 2px solid var(--border);
  font-weight: 600;
  font-size: 11px;
  text-transform: uppercase;
  color: var(--muted);
  background: var(--panel);
}
.overview-table th.sortable {
  cursor: pointer;
  user-select: none;
  transition: background 0.1s;
}
.overview-table th.sortable:hover {
  background: var(--panel2);
}
.overview-table th.numeric {
  text-align: right;
}
.overview-table td {
  padding: 0.6rem 1rem;
  border-bottom: 1px solid var(--border);
}
.overview-table td.numeric {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.overview-table tbody tr {
  transition: background 0.1s;
}
.overview-table tbody tr:hover {
  background: var(--panel2);
}

.bench-name-cell a {
  color: var(--text);
  font-weight: 500;
  text-decoration: none;
  font-family: var(--mono);
  font-size: 13px;
}
.bench-name-cell a:hover {
  color: var(--accent);
  text-decoration: underline;
}

.group-badge.group-micro {
  background: rgba(59, 130, 246, 0.15);
  color: #3b82f6;
}
.group-badge.group-synthetic {
  background: rgba(139, 92, 246, 0.15);
  color: #8b5cf6;
}
.group-badge.group-realistic {
  background: rgba(34, 197, 94, 0.15);
  color: #22c55e;
}
.group-badge.group-other {
  background: var(--panel2);
  color: var(--muted);
}

.ratio-value {
  font-weight: 650;
  font-size: 14px;
}

.ratio-badge {
  margin-left: 8px;
}
.ratio-badge.faster {
  background: rgba(34, 197, 94, 0.15);
  color: var(--good);
}
.ratio-badge.slower {
  background: rgba(239, 68, 68, 0.15);
  color: var(--bad);
}

/* Mobile */
@media (max-width: 768px) {
  .report-sidebar { display: none; }
  .report-nav { flex-direction: column; align-items: stretch; }
  .nav-left, .nav-right { justify-content: center; flex-wrap: wrap; }
  .overview-summary { flex-direction: column; gap: 1rem; }
}
`;

// ============================================================================
// Bootstrap
// ============================================================================

const styleEl = document.createElement('style');
styleEl.textContent = styles;
document.head.appendChild(styleEl);

render(html`<${App} />`, document.getElementById('app'));
