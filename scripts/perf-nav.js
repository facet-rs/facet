// Navigation bar for perf.facet.rs
// Uses Preact + htm for clean component-based UI

import { h, render, Component } from 'https://esm.sh/preact@10.19.3';
import { useState, useEffect, useRef } from 'https://esm.sh/preact@10.19.3/hooks';
import htm from 'https://esm.sh/htm@3.1.1';

const html = htm.bind(h);

// Parse current URL to extract branch and commit
function parseLocation() {
  const path = window.location.pathname;
  const match = path.match(/\/([^/]+)\/([^/]+)\/(report-(deser|ser)\.html)/);
  if (!match) return null;

  return {
    branch: match[1],
    commit: match[2],
    filename: match[3],
    mode: match[4]
  };
}

// Format instruction count with thousand separators
function formatInstructions(n) {
  return n.toLocaleString() + ' instr';
}

// Format delta percentage with color coding
// Negative = improvement (fewer instructions = better)
function formatDelta(deltaPct) {
  const sign = deltaPct >= 0 ? '+' : '';
  const color = deltaPct < 0 ? '#4ade80' : (deltaPct > 0 ? '#f87171' : '#a3adbd');
  const arrow = deltaPct < 0 ? '↓' : (deltaPct > 0 ? '↑' : '');

  return {
    text: `${sign}${deltaPct.toFixed(1)}% ${arrow}`,
    color
  };
}

// Reusable Dropdown component
function Dropdown({ trigger, items, currentValue, onSelect }) {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef(null);

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(e) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target)) {
        setIsOpen(false);
      }
    }

    if (isOpen) {
      document.addEventListener('click', handleClickOutside);
      return () => document.removeEventListener('click', handleClickOutside);
    }
  }, [isOpen]);

  return html`
    <span class="dropdown-container" ref=${dropdownRef}>
      <span
        class="dropdown-trigger"
        onClick=${() => setIsOpen(!isOpen)}
      >
        ${trigger}
      </span>

      ${isOpen && html`
        <div class="dropdown-menu">
          ${items.map(item => html`
            <a
              href="#"
              class="dropdown-item ${item.value === currentValue ? 'active' : ''}"
              onClick=${(e) => {
                e.preventDefault();
                setIsOpen(false);
                onSelect(item.value);
              }}
            >
              <span class="dropdown-label">${item.label}</span>
              ${item.meta && html`
                <span class="dropdown-meta">
                  ${item.metaBefore && html`<span>${item.metaBefore}</span>`}
                  ${item.deltaInfo && html`
                    <span style="color: ${item.deltaInfo.color}">
                      ${item.deltaInfo.text}
                    </span>
                  `}
                  ${item.metaAfter && html`<span>${item.metaAfter}</span>`}
                  ${!item.deltaInfo && !item.metaBefore && !item.metaAfter && item.meta}
                </span>
              `}
            </a>
          `)}
        </div>
      `}
    </span>
  `;
}

// Main navigation component
function NavBar({ location, indexData }) {
  if (!location) return null;

  // Branch dropdown items
  const branchItems = indexData ? Object.keys(indexData.branches).map(b => {
    const commits = indexData.branches[b];
    const latest = commits[0];

    // Show last updated time instead of instruction count
    const meta = latest?.timestamp_display || '';

    return {
      label: b,
      value: b,
      meta: meta ? `${meta}` : `${commits.length} commits`
    };
  }) : [];

  // Commit dropdown items - delta relative to CURRENT commit
  const currentBranchCommits = indexData?.branches[location.branch] || [];
  const currentCommitIndex = currentBranchCommits.findIndex(c => c.commit === location.commit);
  const currentCommit = currentBranchCommits[currentCommitIndex];
  const currentInstructions = currentCommit?.total_instructions;

  const commitItems = currentBranchCommits.map((c, idx) => {
    let metaBefore = c.timestamp_display || '';
    let deltaInfo = null;
    let metaAfter = '';

    // Calculate delta vs CURRENT commit (the one being shown)
    if (currentInstructions && c.total_instructions && idx !== currentCommitIndex) {
      const delta = ((c.total_instructions - currentInstructions) / currentInstructions) * 100;
      deltaInfo = formatDelta(delta);
      if (metaBefore) metaBefore += ' • ';
    } else if (c.total_instructions && idx === currentCommitIndex) {
      // Current commit - just show instruction count
      if (metaBefore) metaBefore += ' • ';
      metaAfter = formatInstructions(c.total_instructions);
    }

    return {
      label: c.commit_short,
      value: c.commit,
      meta: metaBefore || deltaInfo || metaAfter,
      metaBefore,
      deltaInfo,
      metaAfter
    };
  });

  const handleBranchChange = (newBranch) => {
    const commits = indexData.branches[newBranch];
    if (commits && commits.length > 0) {
      const latestCommit = commits[0].commit;
      window.location.href = `/${newBranch}/${latestCommit}/report-${location.mode}.html`;
    }
  };

  const handleCommitChange = (newCommit) => {
    window.location.href = `/${location.branch}/${newCommit}/report-${location.mode}.html`;
  };

  return html`
    <nav id="perf-nav">
      <div class="perf-nav-container">
        <div class="perf-nav-left">
          <a href="/" class="perf-nav-home" title="Home">
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8.354 1.146a.5.5 0 0 0-.708 0l-6 6A.5.5 0 0 0 1.5 7.5v7a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-4h2v4a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.146-.354l-6-6z"/>
            </svg>
            perf.facet.rs
          </a>

          <span class="perf-nav-sep">/</span>

          ${indexData && branchItems.length > 0 ? html`
            <${Dropdown}
              trigger=${html`▼ ${location.branch}`}
              items=${branchItems}
              currentValue=${location.branch}
              onSelect=${handleBranchChange}
            />
          ` : html`
            <span class="perf-nav-branch">${location.branch}</span>
          `}

          <span class="perf-nav-sep">/</span>

          ${indexData && commitItems.length > 0 ? html`
            <${Dropdown}
              trigger=${html`▼ ${location.commit.substring(0, 7)}`}
              items=${commitItems}
              currentValue=${location.commit}
              onSelect=${handleCommitChange}
            />
          ` : html`
            <span class="perf-nav-commit" title=${location.commit}>
              ${location.commit.substring(0, 7)}
            </span>
          `}
        </div>

        <div class="perf-nav-right">
          <a href="/branches.html" class="perf-nav-link">All branches</a>
        </div>
      </div>
    </nav>
  `;
}

// App component that loads data and renders navbar
function App() {
  const [indexData, setIndexData] = useState(null);
  const location = parseLocation();

  useEffect(() => {
    async function loadIndex() {
      try {
        const response = await fetch('/index.json');
        if (response.ok) {
          const data = await response.json();
          setIndexData(data);
        }
      } catch (e) {
        console.error('Failed to load index.json:', e);
      }
    }
    loadIndex();
  }, []);

  return html`
    <${NavBar} location=${location} indexData=${indexData} />
  `;
}

// Styles
const styles = `
#perf-nav {
  position: sticky;
  top: 0;
  left: 0;
  right: 0;
  z-index: 1000;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  font-family: var(--mono);
  font-size: 13px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.perf-nav-container {
  max-width: 1400px;
  margin: 0 auto;
  padding: 10px 16px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.perf-nav-left {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.perf-nav-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.perf-nav-home {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--accent);
  text-decoration: none;
  font-weight: 600;
  transition: opacity 0.15s;
}

.perf-nav-home:hover {
  opacity: 0.8;
}

.perf-nav-sep {
  color: var(--border2);
  font-weight: 400;
}

.perf-nav-branch,
.perf-nav-commit {
  color: var(--muted);
  font-family: var(--mono);
  font-size: 12px;
}

.perf-nav-link {
  color: var(--muted);
  text-decoration: none;
  padding: 4px 8px;
  border-radius: 4px;
  transition: background 0.15s, color 0.15s;
}

.perf-nav-link:hover {
  background: var(--panel2);
  color: var(--text);
}

/* Dropdown styles */
.dropdown-container {
  position: relative;
  display: inline-block;
}

.dropdown-trigger {
  color: var(--text);
  font-weight: 600;
  cursor: pointer;
  padding: 4px 8px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--panel);
  transition: background 0.15s;
  user-select: none;
}

.dropdown-trigger:hover {
  background: var(--panel2);
}

.dropdown-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  z-index: 1001;
  min-width: 250px;
  max-height: 400px;
  overflow-y: auto;
  padding: 4px;
}

.dropdown-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 8px 12px;
  color: var(--text);
  text-decoration: none;
  border-radius: 4px;
  font-size: 13px;
  transition: background 0.1s;
  cursor: pointer;
}

.dropdown-item:hover {
  background: var(--panel2);
}

.dropdown-item.active {
  background: var(--accent);
  color: var(--panel);
}

.dropdown-item.active:hover {
  background: var(--accent);
  opacity: 0.9;
}

.dropdown-label {
  font-weight: 500;
  white-space: nowrap;
}

.dropdown-meta {
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
  text-align: right;
  font-family: var(--mono);
}

.dropdown-item.active .dropdown-meta {
  color: var(--panel);
  opacity: 0.85;
}

body {
  padding-top: 0;
}
`;

// Initialize when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}

function init() {
  // Add styles
  const styleEl = document.createElement('style');
  styleEl.textContent = styles;
  document.head.appendChild(styleEl);

  // Create nav container and render
  const nav = document.createElement('div');
  document.body.insertBefore(nav, document.body.firstChild);
  render(html`<${App} />`, nav);
}
