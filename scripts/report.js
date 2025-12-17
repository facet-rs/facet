// CSR Report Renderer for facet benchmarks
// Renders full benchmark report from run.json + index-v2.json

// ============================================================================
// URL Parsing
// ============================================================================

function parseLocation() {
  var path = window.location.pathname;
  
  // Match: /runs/{branch}/{commit}/report-{op}.html (new format)
  var runsMatch = path.match(/\/runs\/([^/]+)\/([^/]+)\/report-(deser|ser)\.html/);
  if (runsMatch) {
    return {
      branch: decodeURIComponent(runsMatch[1]),
      commit: runsMatch[2],
      operation: runsMatch[3] === 'deser' ? 'deserialize' : 'serialize'
    };
  }

  // Match: /{branch}/{commit}/report-{op}.html (legacy format)
  var legacyMatch = path.match(/\/([^/]+)\/([^/]+)\/report-(deser|ser)\.html/);
  if (legacyMatch) {
    return {
      branch: decodeURIComponent(legacyMatch[1]),
      commit: legacyMatch[2],
      operation: legacyMatch[3] === 'deser' ? 'deserialize' : 'serialize'
    };
  }

  return null;
}

function buildReportUrl(branch, commit, operation) {
  const op = operation === 'serialize' ? 'ser' : 'deser';
  return '/runs/' + encodeURIComponent(branch) + '/' + commit + '/report-' + op + '.html';
}

function buildRunJsonUrl(branch, commit) {
  return '/runs/' + encodeURIComponent(branch) + '/' + commit + '/run.json';
}

// ============================================================================
// Data Loading
// ============================================================================

async function loadRunJson(branch, commit) {
  // Try new location first, then legacy
  const urls = [
    buildRunJsonUrl(branch, commit),
    '/' + encodeURIComponent(branch) + '/' + commit + '/run.json'
  ];

  for (const url of urls) {
    try {
      const resp = await fetch(url);
      if (resp.ok) return await resp.json();
    } catch (e) {
      continue;
    }
  }
  throw new Error('Failed to load run.json for ' + branch + '/' + commit);
}

async function loadIndex() {
  try {
    // Try v2 first
    let resp = await fetch('/index-v2.json');
    if (resp.ok) return await resp.json();

    // Fall back to v1
    resp = await fetch('/index.json');
    if (resp.ok) return await resp.json();
  } catch (e) {
    console.warn('Failed to load index:', e);
  }
  return null;
}

// ============================================================================
// Formatting Helpers
// ============================================================================

function formatTime(ns) {
  if (ns < 1000) return ns.toFixed(1) + ' ns';
  if (ns < 1000000) return (ns / 1000).toFixed(2) + ' us';
  return (ns / 1000000).toFixed(2) + ' ms';
}

function formatCount(n) {
  if (n < 1000) return n.toString();
  if (n < 1000000) return (n / 1000).toFixed(2) + 'K';
  if (n < 1000000000) return (n / 1000000).toFixed(2) + 'M';
  return (n / 1000000000).toFixed(2) + 'G';
}

function formatMetric(value, metricId) {
  if (value === null || value === undefined) return '-';
  if (metricId === 'time_median_ns') return formatTime(value);
  return formatCount(value);
}

function formatRatio(ratio) {
  if (ratio === null || ratio === undefined) return '-';
  return ratio.toFixed(2) + 'x';
}

// ============================================================================
// DOM Helpers
// ============================================================================

function el(tag, attrs, children) {
  attrs = attrs || {};
  children = children || [];
  const element = document.createElement(tag);
  for (const key in attrs) {
    const value = attrs[key];
    if (key === 'className') element.className = value;
    else if (key === 'textContent') element.textContent = value;
    else if (key === 'innerHTML') element.innerHTML = value;
    else if (key.indexOf('on') === 0) element.addEventListener(key.slice(2).toLowerCase(), value);
    else if (key.indexOf('data') === 0) element.setAttribute(key.replace(/([A-Z])/g, '-$1').toLowerCase(), value);
    else element.setAttribute(key, value);
  }
  for (let i = 0; i < children.length; i++) {
    const child = children[i];
    if (typeof child === 'string') element.appendChild(document.createTextNode(child));
    else if (child) element.appendChild(child);
  }
  return element;
}

// ============================================================================
// Navigation Component
// ============================================================================

function renderTopNav(location, indexData) {
  const nav = el('nav', { className: 'top-nav' });
  const container = el('div', { className: 'top-nav-container' });

  // Left side: home + breadcrumb
  const left = el('div', { className: 'top-nav-left' });

  // Home link
  const home = el('a', { className: 'top-nav-home', href: '/' }, [
    el('span', { innerHTML: '<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M8.354 1.146a.5.5 0 0 0-.708 0l-6 6A.5.5 0 0 0 1.5 7.5v7a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-4h2v4a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.146-.354l-6-6z"/></svg>' }),
    'perf.facet.rs'
  ]);
  left.appendChild(home);
  left.appendChild(el('span', { className: 'top-nav-sep', textContent: '/' }));

  // Branch dropdown
  if (indexData && indexData.branches) {
    var branchItems = [];
    for (var key in indexData.branches) {
      var info = indexData.branches[key];
      branchItems.push({
        value: key,
        label: info.display || key,
        meta: info.last_timestamp ? new Date(info.last_timestamp).toLocaleDateString() : ''
      });
    }
    left.appendChild(renderDropdown(
      location.branch,
      branchItems,
      function(newBranch) {
        var commits = indexData.branch_commits && indexData.branch_commits[newBranch];
        if (commits && commits.length > 0) {
          window.location.href = buildReportUrl(newBranch, commits[0].sha, location.operation);
        }
      }
    ));
  } else {
    left.appendChild(el('span', { className: 'dropdown-trigger', textContent: location.branch }));
  }

  left.appendChild(el('span', { className: 'top-nav-sep', textContent: '/' }));

  // Commit dropdown
  if (indexData && indexData.branch_commits && indexData.branch_commits[location.branch]) {
    var commits = indexData.branch_commits[location.branch];
    var commitItems = commits.map(function(c) {
      return {
        value: c.sha,
        label: c.short,
        meta: c.headline && c.headline.ratio ? formatRatio(c.headline.ratio) : ''
      };
    });
    left.appendChild(renderDropdown(
      location.commit.substring(0, 8),
      commitItems,
      function(newCommit) {
        window.location.href = buildReportUrl(location.branch, newCommit, location.operation);
      },
      location.commit
    ));
  } else {
    left.appendChild(el('span', { className: 'dropdown-trigger', textContent: location.commit.substring(0, 8) }));
  }

  container.appendChild(left);

  // Right side: links
  var right = el('div', { className: 'top-nav-right' });
  right.appendChild(el('a', { className: 'top-nav-link', href: '/', textContent: 'All branches' }));
  container.appendChild(right);

  nav.appendChild(container);
  return nav;
}

function renderDropdown(currentLabel, items, onSelect, currentValue) {
  var wrapper = el('div', { className: 'dropdown' });

  var trigger = el('button', { className: 'dropdown-trigger' }, [
    el('span', { className: 'arrow', textContent: '\u25BC' }),
    ' ',
    currentLabel
  ]);

  var menu = el('div', { className: 'dropdown-menu' });

  for (var i = 0; i < items.length; i++) {
    var item = items[i];
    var isActive = currentValue ? item.value === currentValue : item.label === currentLabel;
    (function(item, isActive) {
      var children = [el('span', { className: 'dropdown-label', textContent: item.label })];
      if (item.meta) children.push(el('span', { className: 'dropdown-meta', textContent: item.meta }));
      var itemEl = el('div', {
        className: 'dropdown-item' + (isActive ? ' active' : ''),
        onClick: function() {
          wrapper.classList.remove('open');
          if (!isActive) onSelect(item.value);
        }
      }, children);
      menu.appendChild(itemEl);
    })(item, isActive);
  }

  trigger.addEventListener('click', function(e) {
    e.stopPropagation();
    wrapper.classList.toggle('open');
  });

  wrapper.appendChild(trigger);
  wrapper.appendChild(menu);

  return wrapper;
}

// Close dropdowns on outside click
document.addEventListener('click', function() {
  var dropdowns = document.querySelectorAll('.dropdown.open');
  for (var i = 0; i < dropdowns.length; i++) {
    dropdowns[i].classList.remove('open');
  }
});

// ============================================================================
// Sidebar Component
// ============================================================================

function renderSidebar(runData, operation) {
  var sidebar = el('aside', { className: 'sidebar', id: 'sidebar' });

  // Header with operation switcher
  var header = el('div', { className: 'sidebar-header' });
  header.appendChild(el('span', { textContent: 'Navigation' }));

  var switcher = el('div', { className: 'operation-switcher' });
  var ops = [
    { id: 'deserialize', label: 'Deser' },
    { id: 'serialize', label: 'Ser' }
  ];

  for (var i = 0; i < ops.length; i++) {
    (function(op) {
      var isActive = operation === op.id;
      var btn = el('button', {
        className: 'op-link' + (isActive ? ' active' : ''),
        textContent: op.label,
        onClick: function() {
          if (!isActive) {
            var loc = parseLocation();
            window.location.href = buildReportUrl(loc.branch, loc.commit, op.id);
          }
        }
      });
      switcher.appendChild(btn);
    })(ops[i]);
  }
  header.appendChild(switcher);
  sidebar.appendChild(header);

  // Groups and benchmarks
  for (var g = 0; g < runData.groups.length; g++) {
    var group = runData.groups[g];
    var section = el('div', { className: 'sidebar-section', dataSection: 'section-' + group.group_id });

    var categoryLink = el('a', {
      className: 'sidebar-category',
      href: '#section-' + group.group_id,
      textContent: group.label
    });
    section.appendChild(categoryLink);

    if (group.description) {
      section.appendChild(el('div', { className: 'sidebar-category-desc', textContent: group.description }));
    }

    var benchmarks = el('div', { className: 'sidebar-benchmarks' });
    for (var c = 0; c < group.cases.length; c++) {
      var cs = group.cases[c];
      var benchId = 'bench-' + cs.case_id;
      benchmarks.appendChild(el('a', {
        className: 'sidebar-benchmark',
        href: '#' + benchId,
        dataBench: benchId,
        textContent: cs.label.replace(/_/g, ' ')
      }));
    }
    section.appendChild(benchmarks);
    sidebar.appendChild(section);
  }

  return sidebar;
}

// ============================================================================
// Report Header
// ============================================================================

function renderHeader(runData, operation) {
  var header = el('header');

  var row = el('div', { className: 'header-row' });
  var opLabel = operation === 'serialize' ? 'serialization' : 'deserialization';
  row.appendChild(el('h1', { textContent: 'facet-json ' + opLabel + ' benchmarks' }));
  header.appendChild(row);

  var meta = el('div', { className: 'meta' });

  var generated = el('span', { className: 'meta-item' });
  generated.appendChild(el('strong', { textContent: 'Generated: ' }));
  generated.appendChild(document.createTextNode(new Date(runData.run.generated_at).toLocaleString()));
  meta.appendChild(generated);

  var commit = el('span', { className: 'meta-item' });
  commit.appendChild(el('strong', { textContent: 'Commit: ' }));
  commit.appendChild(el('a', {
    href: 'https://github.com/' + runData.run.repo + '/commit/' + runData.run.commit,
    target: '_blank',
    textContent: runData.run.commit_short
  }));
  meta.appendChild(commit);

  header.appendChild(meta);
  return header;
}

// ============================================================================
// Legend
// ============================================================================

function renderLegend(runData, operation) {
  var legend = el('div', { className: 'legend' });
  legend.appendChild(el('h3', { textContent: 'Targets' }));

  var items = el('div');

  var targetInfo = {
    'facet_format_jit': 'Format-agnostic JIT (our work!)',
    'facet_json_cranelift': 'JSON-specific JIT',
    'facet_format_json': 'Format-agnostic, no JIT',
    'facet_json': 'JSON-specific, no JIT',
    'serde_json': 'Instruction baseline'
  };

  for (var i = 0; i < runData.schema.targets.length; i++) {
    var target = runData.schema.targets[i];
    var desc = targetInfo[target.id] || '';
    var item = el('span', { className: 'legend-item' });
    item.appendChild(el('strong', { textContent: target.label }));
    if (desc) {
      item.appendChild(document.createTextNode(' \u2014 ' + desc));
    }
    items.appendChild(item);
  }
  legend.appendChild(items);

  var note = el('p', { style: 'margin-top: 0.75rem; font-size: 12px; color: var(--muted);' });
  note.appendChild(el('strong', { textContent: 'Instr. Ratio: ' }));
  note.appendChild(document.createTextNode('Instruction count relative to serde_json. <1.0x = fewer instructions (green), >1.0x = more instructions (red).'));
  legend.appendChild(note);

  return legend;
}

// ============================================================================
// Summary Charts
// ============================================================================

function renderSummaryCharts(runData, operation) {
  var container = document.createDocumentFragment();

  var jitTarget = 'facet_format_jit';
  var serdeTarget = 'serde_json';

  for (var g = 0; g < runData.groups.length; g++) {
    var group = runData.groups[g];
    var chartData = [];

    for (var c = 0; c < group.cases.length; c++) {
      var cs = group.cases[c];
      var caseData = runData.results[cs.case_id];
      if (!caseData) continue;

      var jitResult = caseData.targets && caseData.targets[jitTarget] && caseData.targets[jitTarget].ops && caseData.targets[jitTarget].ops[operation];
      var serdeResult = caseData.targets && caseData.targets[serdeTarget] && caseData.targets[serdeTarget].ops && caseData.targets[serdeTarget].ops[operation];

      if (jitResult && jitResult.ok && jitResult.metrics && jitResult.metrics.instructions) {
        chartData.push({
          benchmark: cs.label.replace(/_/g, ' '),
          target: 'facet-format+jit',
          value: jitResult.metrics.instructions / 1000
        });
      }
      if (serdeResult && serdeResult.ok && serdeResult.metrics && serdeResult.metrics.instructions) {
        chartData.push({
          benchmark: cs.label.replace(/_/g, ' '),
          target: 'serde_json',
          value: serdeResult.metrics.instructions / 1000
        });
      }
    }

    if (chartData.length === 0) continue;

    var section = el('div', { className: 'summary-chart' });
    section.appendChild(el('h2', { textContent: group.label + ': facet-format+jit vs serde_json' }));

    var chartWrapper = el('div', { className: 'summary-chart-wrapper', id: 'summary-chart-' + group.group_id });
    section.appendChild(chartWrapper);
    container.appendChild(section);

    renderSummaryChart(chartWrapper, chartData);
  }

  return container;
}

function renderSummaryChart(container, data) {
  function render() {
    if (!window.Plot) return;

    var width = container.clientWidth || 600;
    var isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    var accentColor = isDark ? '#7aa2f7' : '#2457f5';
    var mutedColor = isDark ? '#6b7280' : '#9ca3af';

    var chart = Plot.plot({
      width: width,
      height: Math.max(200, data.length * 20 + 60),
      marginLeft: 140,
      marginRight: 40,
      x: { label: 'Instructions (K)', grid: true },
      y: { label: null },
      color: {
        domain: ['facet-format+jit', 'serde_json'],
        range: [accentColor, mutedColor],
        legend: true
      },
      marks: [
        Plot.barX(data, {
          x: 'value',
          y: 'benchmark',
          fill: 'target',
          sort: { y: '-x' }
        }),
        Plot.ruleX([0])
      ]
    });

    container.innerHTML = '';
    container.appendChild(chart);
  }

  if (window.Plot) render();
  else window.addEventListener('plot-ready', render);
}

// ============================================================================
// Benchmark Tables and Charts
// ============================================================================

function renderBenchmarkSections(runData, operation) {
  var container = document.createDocumentFragment();
  var baselineTarget = runData.schema.defaults.baseline_target;

  for (var g = 0; g < runData.groups.length; g++) {
    var group = runData.groups[g];

    var sectionHeader = el('div', { className: 'section-header', id: 'section-' + group.group_id });
    sectionHeader.appendChild(el('h2', { textContent: group.label }));
    container.appendChild(sectionHeader);

    for (var c = 0; c < group.cases.length; c++) {
      var cs = group.cases[c];
      var caseData = runData.results[cs.case_id];
      if (!caseData) continue;

      var item = renderBenchmarkItem(cs, caseData, runData.schema, operation, baselineTarget);
      if (item) container.appendChild(item);
    }
  }

  return container;
}

function renderBenchmarkItem(caseInfo, caseData, schema, operation, baselineTarget) {
  var benchId = 'bench-' + caseInfo.case_id;

  var results = [];
  for (var i = 0; i < schema.targets.length; i++) {
    var target = schema.targets[i];
    var targetData = caseData.targets && caseData.targets[target.id] && caseData.targets[target.id].ops && caseData.targets[target.id].ops[operation];
    if (targetData) {
      results.push({
        targetId: target.id,
        targetLabel: target.label,
        kind: target.kind,
        ok: targetData.ok,
        metrics: targetData.ok ? targetData.metrics : null,
        error: targetData.ok ? null : targetData.error
      });
    }
  }

  if (results.length === 0) return null;

  var baselineResult = null;
  for (var i = 0; i < results.length; i++) {
    if (results[i].targetId === baselineTarget && results[i].ok) {
      baselineResult = results[i];
      break;
    }
  }
  var baselineInstructions = baselineResult && baselineResult.metrics && baselineResult.metrics.instructions;

  var okResults = results.filter(function(r) { return r.ok && r.metrics && r.metrics.time_median_ns != null; });
  var fastestTime = Infinity;
  for (var i = 0; i < okResults.length; i++) {
    if (okResults[i].metrics.time_median_ns < fastestTime) {
      fastestTime = okResults[i].metrics.time_median_ns;
    }
  }

  results.sort(function(a, b) {
    if (!a.ok) return 1;
    if (!b.ok) return -1;
    return (a.metrics && a.metrics.time_median_ns || Infinity) - (b.metrics && b.metrics.time_median_ns || Infinity);
  });

  var item = el('div', { className: 'benchmark-item', id: benchId, dataOperation: operation });

  var opLabel = operation === 'serialize' ? 'serialize' : 'deserialize';
  item.appendChild(el('h3', { textContent: caseInfo.label.replace(/_/g, ' ') + ' \u2014 ' + opLabel }));

  var tableChartContainer = el('div', { className: 'table-chart-container' });

  var tableWrapper = el('div', { className: 'table-wrapper' });
  var table = el('table', { id: 'table-' + benchId });

  var thead = el('thead');
  var headerRow = el('tr');
  headerRow.appendChild(el('th', { textContent: 'Target' }));
  headerRow.appendChild(el('th', { className: 'num', textContent: 'Median Time' }));
  headerRow.appendChild(el('th', { className: 'num', textContent: 'Instructions' }));
  headerRow.appendChild(el('th', { className: 'num', textContent: 'Instr. Ratio' }));
  thead.appendChild(headerRow);
  table.appendChild(thead);

  var tbody = el('tbody');
  for (var i = 0; i < results.length; i++) {
    var r = results[i];
    var isFastest = r.ok && r.metrics && r.metrics.time_median_ns === fastestTime;
    var isBaseline = r.targetId === baselineTarget;
    var isJit = r.targetId === 'facet_format_jit';

    var rowClass = '';
    if (!r.ok) rowClass = 'errored';
    else if (isFastest) rowClass = 'fastest';
    else if (isBaseline) rowClass = 'baseline';
    else if (isJit) rowClass = 'jit-highlight';

    var row = el('tr', { className: rowClass, dataTarget: r.targetId });
    row.appendChild(el('td', { textContent: r.targetLabel }));

    if (r.ok) {
      row.appendChild(el('td', { className: 'metric num', textContent: formatTime(r.metrics.time_median_ns) }));
      row.appendChild(el('td', { className: 'metric num', textContent: formatCount(r.metrics.instructions || 0) }));

      var ratioCell = el('td', { className: 'num' });
      if (baselineInstructions && r.metrics.instructions) {
        var ratio = r.metrics.instructions / baselineInstructions;
        var ratioClass = ratio < 0.995 ? 'speedup' : ratio > 1.005 ? 'slowdown' : 'neutral';
        ratioCell.className = 'num ' + ratioClass;

        ratioCell.appendChild(el('span', { className: 'metric', textContent: formatRatio(ratio) }));
        if (ratio < 0.995) {
          ratioCell.appendChild(el('span', { className: 'speed-label', textContent: 'fewer' }));
        } else if (ratio > 1.005) {
          ratioCell.appendChild(el('span', { className: 'speed-label', textContent: 'more' }));
        }
      } else {
        ratioCell.textContent = '-';
      }
      row.appendChild(ratioCell);
    } else {
      row.appendChild(el('td', { className: 'metric num error', textContent: 'error' }));
      row.appendChild(el('td', { className: 'metric num', textContent: '-' }));
      row.appendChild(el('td', { className: 'num', textContent: '-' }));
    }

    (function(benchId, targetId) {
      row.addEventListener('mouseenter', function() { highlightChart(benchId, targetId); });
      row.addEventListener('mouseleave', function() { unhighlightChart(benchId); });
    })(benchId, r.targetId);

    tbody.appendChild(row);
  }
  table.appendChild(tbody);
  tableWrapper.appendChild(table);
  tableChartContainer.appendChild(tableWrapper);

  var chartWrapper = el('div', { className: 'chart-wrapper', id: 'chart-' + benchId });
  tableChartContainer.appendChild(chartWrapper);

  item.appendChild(tableChartContainer);

  var chartData = [];
  for (var i = 0; i < results.length; i++) {
    var r = results[i];
    if (r.ok && r.metrics && r.metrics.instructions) {
      chartData.push({
        target: r.targetId,
        label: r.targetLabel,
        value: r.metrics.instructions / 1000
      });
    }
  }

  renderBenchmarkChart(chartWrapper, chartData, benchId);

  return item;
}

function renderBenchmarkChart(container, data, benchId) {
  function render() {
    if (!window.Plot || data.length === 0) return;

    var width = container.clientWidth || 400;
    var barHeight = 28;

    var chart = Plot.plot({
      width: width,
      height: data.length * barHeight + 50,
      marginLeft: 130,
      marginRight: 20,
      marginTop: 10,
      marginBottom: 40,
      x: { label: 'Instructions (K)', grid: true },
      y: { label: null },
      marks: [
        Plot.barX(data, {
          x: 'value',
          y: 'label',
          sort: { y: 'x' }
        }),
        Plot.ruleX([0])
      ]
    });

    container.innerHTML = '';
    container.appendChild(chart);

    var bars = chart.querySelectorAll('rect');
    var sortedData = data.slice().sort(function(a, b) { return a.value - b.value; });
    for (var i = 0; i < sortedData.length; i++) {
      if (bars[i]) bars[i].setAttribute('data-target', sortedData[i].target);
    }
  }

  if (window.Plot) render();
  else window.addEventListener('plot-ready', render);
}

// ============================================================================
// Chart Highlighting
// ============================================================================

function highlightChart(benchId, targetId) {
  var chart = document.getElementById('chart-' + benchId);
  var table = document.getElementById('table-' + benchId);

  if (table) {
    var rows = table.querySelectorAll('tbody tr');
    for (var i = 0; i < rows.length; i++) {
      if (rows[i].dataset.target === targetId) {
        rows[i].classList.remove('dimmed');
      } else {
        rows[i].classList.add('dimmed');
      }
    }
  }

  if (chart) {
    var bars = chart.querySelectorAll('rect[data-target]');
    for (var i = 0; i < bars.length; i++) {
      var isTarget = bars[i].getAttribute('data-target') === targetId;
      if (isTarget) {
        bars[i].classList.add('highlighted');
        bars[i].classList.remove('dimmed');
      } else {
        bars[i].classList.add('dimmed');
        bars[i].classList.remove('highlighted');
      }
    }
  }
}

function unhighlightChart(benchId) {
  var chart = document.getElementById('chart-' + benchId);
  var table = document.getElementById('table-' + benchId);

  if (table) {
    var rows = table.querySelectorAll('tbody tr');
    for (var i = 0; i < rows.length; i++) {
      rows[i].classList.remove('dimmed');
    }
  }

  if (chart) {
    var bars = chart.querySelectorAll('rect[data-target]');
    for (var i = 0; i < bars.length; i++) {
      bars[i].classList.remove('highlighted', 'dimmed');
    }
  }
}

// ============================================================================
// Footer
// ============================================================================

function renderFooter() {
  var footer = el('footer');

  var p1 = el('p');
  p1.appendChild(el('strong', { textContent: 'Generated by ' }));
  p1.appendChild(document.createTextNode('benchmark-analyzer'));
  footer.appendChild(p1);

  footer.appendChild(el('p', { textContent: 'Benchmark tools: divan (wall-clock) + gungraun (instruction counts)' }));

  return footer;
}

// ============================================================================
// Sidebar Scroll Highlighting
// ============================================================================

function setupScrollHighlighting() {
  var sidebar = document.getElementById('sidebar');
  if (!sidebar) return;

  var sectionHeaders = document.querySelectorAll('.section-header[id]');
  var benchmarkItems = document.querySelectorAll('.benchmark-item[id]');
  var sidebarCategories = sidebar.querySelectorAll('.sidebar-category');
  var sidebarBenchmarks = sidebar.querySelectorAll('.sidebar-benchmark');

  var sectionMap = {};
  for (var i = 0; i < sidebarCategories.length; i++) {
    var link = sidebarCategories[i];
    var href = link.getAttribute('href');
    if (href && href.indexOf('#') === 0) sectionMap[href.slice(1)] = link;
  }

  var benchMap = {};
  for (var i = 0; i < sidebarBenchmarks.length; i++) {
    var link = sidebarBenchmarks[i];
    var benchId = link.dataset.bench;
    if (benchId) benchMap[benchId] = link;
  }

  function update() {
    var offset = 100;

    var activeSection = null;
    for (var i = 0; i < sectionHeaders.length; i++) {
      if (sectionHeaders[i].getBoundingClientRect().top <= offset) {
        activeSection = sectionHeaders[i].id;
      }
    }

    var activeBench = null;
    var closestDist = Infinity;
    for (var i = 0; i < benchmarkItems.length; i++) {
      var rect = benchmarkItems[i].getBoundingClientRect();
      if (rect.top <= offset && rect.bottom > 0) {
        var dist = Math.abs(rect.top - offset);
        if (dist < closestDist) {
          closestDist = dist;
          activeBench = benchmarkItems[i].id;
        }
      }
    }

    for (var i = 0; i < sidebarCategories.length; i++) sidebarCategories[i].classList.remove('active');
    for (var i = 0; i < sidebarBenchmarks.length; i++) sidebarBenchmarks[i].classList.remove('active');

    if (activeSection && sectionMap[activeSection]) {
      sectionMap[activeSection].classList.add('active');
    }
    if (activeBench && benchMap[activeBench]) {
      var link = benchMap[activeBench];
      link.classList.add('active');

      var sidebarRect = sidebar.getBoundingClientRect();
      var linkRect = link.getBoundingClientRect();
      if (linkRect.top < sidebarRect.top || linkRect.bottom > sidebarRect.bottom) {
        link.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      }
    }
  }

  var ticking = false;
  window.addEventListener('scroll', function() {
    if (!ticking) {
      requestAnimationFrame(function() {
        update();
        ticking = false;
      });
      ticking = true;
    }
  });

  update();
}

// ============================================================================
// Main Render
// ============================================================================

async function render() {
  var app = document.getElementById('app');
  var location = parseLocation();

  if (!location) {
    app.innerHTML = '<div class="loading">Invalid URL. Expected format: /{branch}/{commit}/report-deser.html</div>';
    return;
  }

  try {
    var results = await Promise.all([
      loadRunJson(location.branch, location.commit),
      loadIndex()
    ]);
    var runData = results[0];
    var indexData = results[1];

    app.innerHTML = '';

    var layout = el('div', { className: 'report-layout' });

    app.appendChild(renderTopNav(location, indexData));

    layout.appendChild(renderSidebar(runData, location.operation));

    var main = el('main', { className: 'main-content' });
    var container = el('div', { className: 'container' });

    container.appendChild(renderHeader(runData, location.operation));
    container.appendChild(renderSummaryCharts(runData, location.operation));
    container.appendChild(renderLegend(runData, location.operation));
    container.appendChild(renderBenchmarkSections(runData, location.operation));
    container.appendChild(renderFooter());

    main.appendChild(container);
    layout.appendChild(main);
    app.appendChild(layout);

    var opLabel = location.operation === 'serialize' ? 'serialization' : 'deserialization';
    document.title = 'facet-json ' + opLabel + ' benchmarks';

    setupScrollHighlighting();

  } catch (e) {
    console.error('Failed to render report:', e);
    app.innerHTML = '<div class="loading">Failed to load benchmark data: ' + e.message + '</div>';
  }
}

// Initialize
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', render);
} else {
  render();
}
