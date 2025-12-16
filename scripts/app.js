// Single-page app for perf.facet.rs
// Hash-based routing: #/ = home, #/branches = all branches

import { h, render } from 'https://esm.sh/preact@10.19.3';
import { useState, useEffect } from 'https://esm.sh/preact@10.19.3/hooks';
import htm from 'https://esm.sh/htm@3.1.1';

const html = htm.bind(h);

function formatNumber(n) {
  return n.toLocaleString();
}

function formatTimestamp(iso) {
  if (!iso) return '—';
  try {
    const date = new Date(iso);
    return new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      hour12: true
    }).format(date);
  } catch (e) {
    return iso;
  }
}

// Homepage view
function HomePage({ data }) {
  const mainBranch = data.branches.main || [];
  const latestMain = mainBranch[0];

  // Recent branches (last 7 days, excluding main)
  const now = Date.now() / 1000;
  const sevenDays = 7 * 24 * 60 * 60;
  const recentBranches = Object.keys(data.branches)
    .filter(name => name !== 'main')
    .map(name => ({
      name,
      commits: data.branches[name],
      latestTimestamp: data.branches[name][0]?.timestamp
    }))
    .filter(b => {
      if (!b.latestTimestamp) return false;
      const ts = new Date(b.latestTimestamp).getTime() / 1000;
      return now - ts < sevenDays;
    })
    .slice(0, 5);

  return html`
    <h1>facet performance benchmarks</h1>
    <p>
      Automated benchmark results published from CI.
      <a href="#/branches">View all branches →</a>
    </p>

    ${latestMain && html`
      <div class="card">
        <h2>Latest: <code>${latestMain.commit_short}</code></h2>
        <div>
          <a class="button" href="/main/${latestMain.commit}/report-deser.html">
            Deserialization →
          </a>
          <a class="button" href="/main/${latestMain.commit}/report-ser.html">
            Serialization →
          </a>
        </div>
        <div class="meta">Branch: main</div>
        ${latestMain.total_instructions && html`
          <div class="meta">
            Instructions: ${formatNumber(latestMain.total_instructions)}
          </div>
        `}
      </div>
    `}

    ${recentBranches.length > 0 && html`
      <div class="card">
        <h2>Recent Activity</h2>
        <p style="color: var(--muted); margin-bottom: 1em;">
          Branches with commits in the last 7 days
        </p>

        ${recentBranches.map(branch => html`
          <div key=${branch.name} style="margin: 1em 0; padding: 1em; background: var(--panel2); border-radius: 6px;">
            <h3 style="margin-bottom: 0.5em; font-size: 15px;">${branch.name}</h3>
            <ul style="list-style: none; padding: 0;">
              ${branch.commits.slice(0, 2).map(commit => html`
                <li key=${commit.commit} style="margin: 0.5em 0;">
                  <a href="/${branch.name}/${commit.commit}/report-deser.html">
                    <code>${commit.commit_short}</code>
                  </a>
                  ${commit.timestamp && html`
                    <span style="color: var(--muted); margin-left: 0.5em;">
                      ${formatTimestamp(commit.timestamp)}
                    </span>
                  `}
                  <span style="margin-left: 0.5em;">
                    <a href="/${branch.name}/${commit.commit}/report-deser.html">deser</a>
                    ${' | '}
                    <a href="/${branch.name}/${commit.commit}/report-ser.html">ser</a>
                  </span>
                  ${commit.total_instructions && html`
                    <span style="color: var(--muted); margin-left: 0.5em; font-size: 12px;">
                      (${formatNumber(commit.total_instructions)} instr)
                    </span>
                  `}
                </li>
              `)}
            </ul>
          </div>
        `)}
      </div>
    `}

    <div class="card">
      <h3>About</h3>
      <p>These benchmarks measure JSON deserialization and serialization performance across different facet implementations:</p>
      <ul>
        <li><strong>facet-format+jit</strong>: Format-agnostic JIT compiler (our main innovation)</li>
        <li><strong>facet-json+jit</strong>: JSON-specific JIT using Cranelift</li>
        <li><strong>facet-format</strong>: Format-agnostic interpreter</li>
        <li><strong>facet-json</strong>: JSON-specific interpreter</li>
        <li><strong>serde_json</strong>: Baseline comparison</li>
      </ul>
    </div>
  `;
}

// Branch row component
function BranchRow({ name, commits, expanded, onToggle }) {
  const latestCommit = commits[0];
  const isStale = latestCommit?.timestamp
    ? (Date.now() / 1000 - new Date(latestCommit.timestamp).getTime() / 1000) > (90 * 24 * 60 * 60)
    : false;

  return html`
    <div class="branch-section">
      <h2 style="cursor: pointer; user-select: none;" onClick=${onToggle}>
        <span style="display: inline-block; width: 20px;">
          ${expanded ? '▼' : '▶'}
        </span>
        ${name}
        <span style="color: var(--muted); font-size: 14px; font-weight: 400;">
          ${isStale && '(stale, '}
          (${commits.length} commit${commits.length !== 1 ? 's' : ''})
          ${isStale && ')'}
        </span>
      </h2>

      ${expanded && html`
        <table>
          <thead>
            <tr>
              <th>Commit</th>
              <th>Branch</th>
              <th>PR</th>
              <th>Generated</th>
              <th>Instructions</th>
              <th>Reports</th>
            </tr>
          </thead>
          <tbody>
            ${commits.map(commit => html`
              <tr key=${commit.commit}>
                <td>
                  <a href=${'https://github.com/facet-rs/facet/commit/' + commit.commit} target="_blank">
                    <code>${commit.commit_short}</code>
                  </a>
                </td>
                <td>
                  <a href=${'https://github.com/facet-rs/facet/tree/' + commit.branch_original} target="_blank">
                    ${commit.branch_original}
                  </a>
                </td>
                <td>
                  ${commit.pr_number
                    ? html`<a href=${'https://github.com/facet-rs/facet/pull/' + commit.pr_number} target="_blank">#${commit.pr_number}</a>`
                    : '—'
                  }
                </td>
                <td title=${commit.timestamp}>
                  ${formatTimestamp(commit.timestamp)}
                </td>
                <td>
                  ${commit.total_instructions
                    ? html`<code style="font-size: 12px;">${formatNumber(commit.total_instructions)}</code>`
                    : '—'
                  }
                </td>
                <td>
                  <a href=${'/' + name + '/' + commit.commit + '/report-deser.html'}>deserialize</a>
                  ${' | '}
                  <a href=${'/' + name + '/' + commit.commit + '/report-ser.html'}>serialize</a>
                </td>
              </tr>
            `)}
          </tbody>
        </table>
      `}
    </div>
  `;
}

// All branches view
function BranchesPage({ data }) {
  const [filter, setFilter] = useState('');
  const [expandedBranches, setExpandedBranches] = useState(new Set(['main']));

  // Get all branches and categorize them
  const now = Date.now() / 1000;
  const ninetyDays = 90 * 24 * 60 * 60;

  const allBranches = Object.keys(data.branches)
    .map(name => ({
      name,
      commits: data.branches[name],
      latestTimestamp: data.branches[name][0]?.timestamp
    }))
    .map(b => ({
      ...b,
      isStale: b.latestTimestamp
        ? (now - new Date(b.latestTimestamp).getTime() / 1000) > ninetyDays
        : false
    }))
    .filter(b => {
      if (!filter) return true;
      const filterLower = filter.toLowerCase();
      return b.name.toLowerCase().includes(filterLower) ||
             b.commits.some(c =>
               c.commit.toLowerCase().includes(filterLower) ||
               c.commit_short.toLowerCase().includes(filterLower) ||
               c.branch_original.toLowerCase().includes(filterLower)
             );
    });

  // Separate main, active, and stale
  const mainBranch = allBranches.find(b => b.name === 'main');
  const otherBranches = allBranches.filter(b => b.name !== 'main');
  const activeBranches = otherBranches.filter(b => !b.isStale);
  const staleBranches = otherBranches.filter(b => b.isStale);

  const toggleBranch = (name) => {
    setExpandedBranches(prev => {
      const newSet = new Set(prev);
      if (newSet.has(name)) {
        newSet.delete(name);
      } else {
        newSet.add(name);
      }
      return newSet;
    });
  };

  const expandAll = () => {
    setExpandedBranches(new Set(allBranches.map(b => b.name)));
  };

  const collapseAll = () => {
    setExpandedBranches(new Set());
  };

  return html`
    <h1>facet benchmarks - all branches</h1>
    <p><a href="#/">← Back to latest main</a></p>

    <div style="margin: 1em 0; display: flex; gap: 1em; align-items: center; flex-wrap: wrap;">
      <input
        type="text"
        placeholder="Filter branches..."
        value=${filter}
        onInput=${(e) => setFilter(e.target.value)}
        style="
          flex: 1;
          min-width: 200px;
          padding: 0.5em 0.75em;
          background: var(--panel);
          border: 1px solid var(--border);
          border-radius: 4px;
          color: var(--text);
          font-family: var(--mono);
          font-size: 13px;
        "
      />
      <button onClick=${expandAll} class="filter-button">
        Expand all
      </button>
      <button onClick=${collapseAll} class="filter-button">
        Collapse all
      </button>
    </div>

    ${mainBranch && html`
      <${BranchRow}
        name=${mainBranch.name}
        commits=${mainBranch.commits}
        expanded=${expandedBranches.has(mainBranch.name)}
        onToggle=${() => toggleBranch(mainBranch.name)}
      />
    `}

    ${activeBranches.map(branch => html`
      <${BranchRow}
        key=${branch.name}
        name=${branch.name}
        commits=${branch.commits}
        expanded=${expandedBranches.has(branch.name)}
        onToggle=${() => toggleBranch(branch.name)}
      />
    `)}

    ${staleBranches.length > 0 && html`
      <details style="margin-top: 2em;">
        <summary style="
          cursor: pointer;
          padding: 1em;
          background: var(--panel);
          border: 1px solid var(--border);
          border-radius: 8px;
          font-weight: 600;
        ">
          Stale branches (no commits in last 90 days) — ${staleBranches.length} branch${staleBranches.length !== 1 ? 'es' : ''}
        </summary>
        ${staleBranches.map(branch => html`
          <${BranchRow}
            key=${branch.name}
            name=${branch.name}
            commits=${branch.commits}
            expanded=${expandedBranches.has(branch.name)}
            onToggle=${() => toggleBranch(branch.name)}
          />
        `)}
      </details>
    `}
  `;
}

// Main app with routing
function App() {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [route, setRoute] = useState(window.location.hash || '#/');

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

    // Listen for hash changes
    const handleHashChange = () => {
      setRoute(window.location.hash || '#/');
    };
    window.addEventListener('hashchange', handleHashChange);
    return () => window.removeEventListener('hashchange', handleHashChange);
  }, []);

  if (loading) {
    return html`
      <div style="text-align: center; padding: 4em 1em; color: var(--muted);">
        Loading...
      </div>
    `;
  }

  if (error) {
    return html`
      <div style="text-align: center; padding: 4em 1em; color: var(--muted);">
        Error: ${error}
      </div>
    `;
  }

  // Route to the appropriate view
  if (route === '#/branches') {
    return html`<${BranchesPage} data=${data} />`;
  }

  return html`<${HomePage} data=${data} />`;
}

// Styles
const styles = `
table {
  width: 100%;
  border-collapse: collapse;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
  margin-top: 1em;
}

th, td {
  text-align: left;
  padding: 0.75em;
  border-bottom: 1px solid var(--border);
}

th {
  background: var(--panel2);
  font-weight: 600;
  font-size: 13px;
}

tbody tr:last-child td {
  border-bottom: none;
}

tbody tr:hover {
  background: var(--panel2);
}

.branch-section {
  background: var(--panel);
  margin: 1em 0;
  padding: 1em;
  border-radius: 8px;
  border: 1px solid var(--border);
}

details summary:hover {
  background: var(--panel2);
}

input:focus, button:focus {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.filter-button {
  padding: 0.5em 1em;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text);
  cursor: pointer;
  font-family: var(--mono);
  font-size: 13px;
}

.filter-button:hover {
  background: var(--panel2);
}
`;

// Add styles
const styleEl = document.createElement('style');
styleEl.textContent = styles;
document.head.appendChild(styleEl);

// Render the app
render(html`<${App} />`, document.getElementById('app'));
