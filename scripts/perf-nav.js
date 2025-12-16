// Navigation bar for perf.facet.rs
// Injected into all benchmark reports for easy navigation

(function() {
  'use strict';

  let indexData = null;  // Will hold the parsed index.json

  // Parse current URL to extract branch and commit
  function parseLocation() {
    const path = window.location.pathname;
    // Path format: /perf.facet.rs/{branch}/{commit}/report-{deser|ser}.html
    const match = path.match(/\/([^/]+)\/([^/]+)\/(report-(deser|ser)\.html)/);
    if (!match) return null;

    return {
      branch: match[1],
      commit: match[2],
      filename: match[3],
      mode: match[4]
    };
  }

  // Fetch index.json with the branch/commit data
  async function loadIndex() {
    try {
      const response = await fetch('/index.json');
      if (!response.ok) return null;
      indexData = await response.json();
      return indexData;
    } catch (e) {
      console.error('Failed to load index.json:', e);
      return null;
    }
  }

  // Format instruction count with thousand separators
  function formatInstructions(n) {
    return n.toLocaleString() + ' instr';
  }

  // Format delta percentage with color coding
  // Negative = improvement (fewer instructions = better)
  // Returns an object with {text, color} instead of HTML
  function formatDelta(deltaPct) {
    const sign = deltaPct >= 0 ? '+' : '';
    const color = deltaPct < 0 ? '#4ade80' : (deltaPct > 0 ? '#f87171' : '#a3adbd');
    const arrow = deltaPct < 0 ? '↓' : (deltaPct > 0 ? '↑' : '');

    return {
      text: `${sign}${deltaPct.toFixed(1)}% ${arrow}`,
      color
    };
  }

  // Create a dropdown menu
  function createDropdown(items, currentValue, onSelect) {
    const dropdown = document.createElement('div');
    dropdown.className = 'perf-nav-dropdown';
    dropdown.style.display = 'none';

    const list = document.createElement('div');
    list.className = 'perf-nav-dropdown-list';

    items.forEach(item => {
      const option = document.createElement('a');
      option.href = '#';
      option.className = 'perf-nav-dropdown-item';
      if (item.value === currentValue) {
        option.classList.add('active');
      }

      const labelSpan = document.createElement('span');
      labelSpan.textContent = item.label;
      option.appendChild(labelSpan);

      if (item.meta) {
        const metaSpan = document.createElement('span');
        metaSpan.className = 'perf-nav-dropdown-meta';

        // If there's delta info, apply color to just the delta part
        if (item.deltaInfo) {
          const parts = item.meta.split(item.deltaInfo.text);
          metaSpan.textContent = parts[0];

          const deltaSpan = document.createElement('span');
          deltaSpan.textContent = item.deltaInfo.text;
          deltaSpan.style.color = item.deltaInfo.color;
          metaSpan.appendChild(deltaSpan);

          if (parts[1]) {
            const restSpan = document.createTextNode(parts[1]);
            metaSpan.appendChild(restSpan);
          }
        } else {
          metaSpan.textContent = item.meta;
        }

        option.appendChild(metaSpan);
      }

      option.addEventListener('click', (e) => {
        e.preventDefault();
        dropdown.style.display = 'none';
        onSelect(item.value);
      });

      list.appendChild(option);
    });

    dropdown.appendChild(list);
    return dropdown;
  }

  // Create the navigation bar using safe DOM methods
  function createNavBar() {
    const loc = parseLocation();
    if (!loc) return;

    // Create nav element
    const nav = document.createElement('nav');
    nav.id = 'perf-nav';

    // Create container
    const container = document.createElement('div');
    container.className = 'perf-nav-container';

    // Left side
    const leftDiv = document.createElement('div');
    leftDiv.className = 'perf-nav-left';

    // Home link with icon
    const homeLink = document.createElement('a');
    homeLink.href = '/';
    homeLink.className = 'perf-nav-home';
    homeLink.title = 'Home';

    const homeIcon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    homeIcon.setAttribute('width', '16');
    homeIcon.setAttribute('height', '16');
    homeIcon.setAttribute('viewBox', '0 0 16 16');
    homeIcon.setAttribute('fill', 'currentColor');

    const homePath = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    homePath.setAttribute('d', 'M8.354 1.146a.5.5 0 0 0-.708 0l-6 6A.5.5 0 0 0 1.5 7.5v7a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-4h2v4a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.146-.354l-6-6z');

    homeIcon.appendChild(homePath);
    homeLink.appendChild(homeIcon);
    homeLink.appendChild(document.createTextNode('perf.facet.rs'));

    // Separator 1
    const sep1 = document.createElement('span');
    sep1.className = 'perf-nav-sep';
    sep1.textContent = '/';

    // Branch name
    const branchSpan = document.createElement('span');
    branchSpan.className = 'perf-nav-branch';
    branchSpan.textContent = loc.branch;

    // Separator 2
    const sep2 = document.createElement('span');
    sep2.className = 'perf-nav-sep';
    sep2.textContent = '/';

    // Commit hash (will be made interactive later)
    const commitSpan = document.createElement('span');
    commitSpan.className = 'perf-nav-commit';
    commitSpan.title = loc.commit;
    commitSpan.textContent = loc.commit.substring(0, 7);

    // Make branch interactive if we have index data
    const branchContainer = document.createElement('span');
    branchContainer.style.position = 'relative';
    branchContainer.style.display = 'inline-block';

    const commitContainer = document.createElement('span');
    commitContainer.style.position = 'relative';
    commitContainer.style.display = 'inline-block';

    leftDiv.appendChild(homeLink);
    leftDiv.appendChild(sep1);
    leftDiv.appendChild(branchContainer);
    branchContainer.appendChild(branchSpan);
    leftDiv.appendChild(sep2);
    leftDiv.appendChild(commitContainer);
    commitContainer.appendChild(commitSpan);

    // Load index data and make interactive
    loadIndex().then(data => {
      if (!data || !data.branches) return;

      // Make branch clickable
      branchSpan.style.cursor = 'pointer';
      branchSpan.style.textDecoration = 'underline';
      branchSpan.style.textDecorationStyle = 'dotted';

      const branchItems = Object.keys(data.branches).map(b => {
        const commits = data.branches[b];
        const latest = commits[0];
        let meta = `${commits.length} commits`;

        // Add instruction count if available
        if (latest && latest.total_instructions) {
          meta += ` • ${formatInstructions(latest.total_instructions)}`;
        }

        return {
          label: b,
          value: b,
          meta
        };
      });

      const branchDropdown = createDropdown(branchItems, loc.branch, (newBranch) => {
        // Switch to the latest commit on the new branch, same mode
        const commits = data.branches[newBranch];
        if (commits && commits.length > 0) {
          const latestCommit = commits[0].commit;
          window.location.href = `/${newBranch}/${latestCommit}/report-${loc.mode}.html`;
        }
      });

      branchContainer.appendChild(branchDropdown);
      branchSpan.addEventListener('click', () => {
        branchDropdown.style.display = branchDropdown.style.display === 'none' ? 'block' : 'none';
      });

      // Make commit clickable
      const currentBranchCommits = data.branches[loc.branch];
      if (currentBranchCommits && currentBranchCommits.length > 0) {
        commitSpan.style.cursor = 'pointer';
        commitSpan.style.textDecoration = 'underline';
        commitSpan.style.textDecorationStyle = 'dotted';

        const commitItems = currentBranchCommits.map((c, idx) => {
          let meta = c.timestamp_display || '';
          let deltaInfo = null;

          // Calculate delta vs previous commit (in JS!)
          if (c.total_instructions && idx + 1 < currentBranchCommits.length) {
            const prev = currentBranchCommits[idx + 1];
            if (prev && prev.total_instructions) {
              const delta = ((c.total_instructions - prev.total_instructions) / prev.total_instructions) * 100;
              deltaInfo = formatDelta(delta);

              if (meta) meta += ' • ';
              meta += deltaInfo.text;
            }
          } else if (c.total_instructions) {
            // No delta, just show instruction count
            if (meta) meta += ' • ';
            meta += formatInstructions(c.total_instructions);
          }

          return {
            label: c.commit_short,
            value: c.commit,
            meta,
            deltaInfo  // Store separately for coloring
          };
        });

        const commitDropdown = createDropdown(commitItems, loc.commit, (newCommit) => {
          // Switch to the new commit, same mode
          window.location.href = `/${loc.branch}/${newCommit}/report-${loc.mode}.html`;
        });

        commitContainer.appendChild(commitDropdown);
        commitSpan.addEventListener('click', () => {
          commitDropdown.style.display = commitDropdown.style.display === 'none' ? 'block' : 'none';
        });
      }

      // Close dropdowns when clicking outside
      document.addEventListener('click', (e) => {
        if (!branchContainer.contains(e.target)) {
          branchDropdown.style.display = 'none';
        }
        if (!commitContainer.contains(e.target) && currentBranchCommits) {
          const commitDropdown = commitContainer.querySelector('.perf-nav-dropdown');
          if (commitDropdown) {
            commitDropdown.style.display = 'none';
          }
        }
      });
    });

    // Right side
    const rightDiv = document.createElement('div');
    rightDiv.className = 'perf-nav-right';

    const branchesLink = document.createElement('a');
    branchesLink.href = '/branches.html';
    branchesLink.className = 'perf-nav-link';
    branchesLink.textContent = 'All branches';

    rightDiv.appendChild(branchesLink);

    // Assemble
    container.appendChild(leftDiv);
    container.appendChild(rightDiv);
    nav.appendChild(container);

    // Add styles
    const style = document.createElement('style');
    style.textContent = `
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

      .perf-nav-branch {
        color: var(--text);
        font-weight: 600;
      }

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
      .perf-nav-dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: 4px;
        background: var(--panel);
        border: 1px solid var(--border);
        border-radius: 6px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        z-index: 1001;
        min-width: 200px;
        max-height: 400px;
        overflow-y: auto;
      }

      .perf-nav-dropdown-list {
        padding: 4px;
      }

      .perf-nav-dropdown-item {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 12px;
        padding: 6px 10px;
        color: var(--text);
        text-decoration: none;
        border-radius: 4px;
        font-size: 13px;
        transition: background 0.1s;
      }

      .perf-nav-dropdown-item:hover {
        background: var(--panel2);
      }

      .perf-nav-dropdown-item.active {
        background: var(--accent);
        color: var(--panel);
      }

      .perf-nav-dropdown-item.active:hover {
        background: var(--accent);
        opacity: 0.9;
      }

      .perf-nav-dropdown-meta {
        color: var(--muted);
        font-size: 11px;
        white-space: nowrap;
      }

      .perf-nav-dropdown-item.active .perf-nav-dropdown-meta {
        color: var(--panel);
        opacity: 0.8;
      }

      /* Adjust body padding to account for navbar */
      body {
        padding-top: 0;
      }
    `;

    document.head.appendChild(style);
    document.body.insertBefore(nav, document.body.firstChild);
  }

  // Initialize when DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', createNavBar);
  } else {
    createNavBar();
  }
})();
