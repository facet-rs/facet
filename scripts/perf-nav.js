// Navigation bar for perf.facet.rs
// Injected into all benchmark reports for easy navigation

(function() {
  'use strict';

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

    // Commit hash
    const commitSpan = document.createElement('span');
    commitSpan.className = 'perf-nav-commit';
    commitSpan.title = loc.commit;
    commitSpan.textContent = loc.commit.substring(0, 7);

    leftDiv.appendChild(homeLink);
    leftDiv.appendChild(sep1);
    leftDiv.appendChild(branchSpan);
    leftDiv.appendChild(sep2);
    leftDiv.appendChild(commitSpan);

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
