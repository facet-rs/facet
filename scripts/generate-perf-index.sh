#!/usr/bin/env bash
# Generate index.html and branches.html for perf.facet.rs
set -euo pipefail

PERF_DIR="${1:?Usage: $0 <perf-directory>}"
cd "$PERF_DIR"

# Helper function to read JSON field
get_json_field() {
  local json_file="$1"
  local field="$2"
  grep "\"$field\"" "$json_file" | sed 's/.*": "\(.*\)".*/\1/' || echo ""
}

# Collect branches and commits with metadata
declare -A branches
declare -A commit_metadata  # Store full metadata for each commit

for branch_dir in */; do
  branch="${branch_dir%/}"
  [[ "$branch" == "fonts" ]] && continue
  [[ ! -d "$branch_dir" ]] && continue

  commits=()
  for commit_dir in "$branch_dir"*/; do
    commit="${commit_dir%/}"
    commit="${commit##*/}"
    [[ "$commit" == "latest" ]] && continue

    # Try to read metadata.json
    metadata_file="${commit_dir}metadata.json"
    if [[ -f "$metadata_file" ]]; then
      timestamp_iso=$(get_json_field "$metadata_file" "timestamp")
      branch_orig=$(get_json_field "$metadata_file" "branch_original")
      pr_number=$(get_json_field "$metadata_file" "pr_number")
      timestamp_display=$(get_json_field "$metadata_file" "timestamp_display")
      commit_short=$(get_json_field "$metadata_file" "commit_short")

      # Convert ISO timestamp to Unix timestamp for numeric sorting
      timestamp=$(date -d "$timestamp_iso" +%s 2>/dev/null || echo 0)

      # Store metadata for later use (branch/commit as key)
      commit_metadata["$branch/$commit"]="$branch_orig|$pr_number|$timestamp_display|$commit_short"
      commits+=("$commit:$timestamp")
    else
      # Fallback to git commit timestamp (if available) or directory modification time
      timestamp=$(git log -1 --format=%ct "$commit" 2>/dev/null || stat -c %Y "$commit_dir" 2>/dev/null || echo 0)
      commits+=("$commit:$timestamp")
    fi
  done

  IFS=$'\n' sorted=($(sort -t: -k2 -rn <<<"${commits[*]}" || true))
  branches["$branch"]="${sorted[*]}"
done

# Generate index.html (frontpage with latest main)
cat > index.html <<'EOF'
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>facet benchmarks</title>
  <link rel="icon" href="/favicon.png" sizes="32x32" type="image/png">
  <link rel="icon" href="/favicon.ico" type="image/x-icon">
  <link rel="apple-touch-icon" href="/favicon.png">
  <style>
@font-face {
  font-family: 'Iosevka FTL';
  src: url('fonts/IosevkaFtl-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'Iosevka FTL';
  src: url('fonts/IosevkaFtl-Bold.ttf') format('truetype');
  font-weight: 600 700;
  font-style: normal;
  font-display: swap;
}

:root {
  color-scheme: light dark;

  --mono: 'Iosevka FTL', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;

  /* Surfaces */
  --bg:     light-dark(#fbfbfc, #0b0e14);
  --panel:  light-dark(#ffffff, #0f1420);
  --panel2: light-dark(#f6f7f9, #0c111b);

  /* Text */
  --text:  light-dark(#0e1116, #e7eaf0);
  --muted: light-dark(#3a4556, #a3adbd);

  /* Borders */
  --border:  light-dark(rgba(0,0,0,0.1), rgba(255,255,255,0.1));

  /* Semantic */
  --accent: light-dark(#2457f5, #7aa2f7);
}

* { margin: 0; padding: 0; box-sizing: border-box; }

body {
  font-family: var(--mono);
  background: var(--bg);
  color: var(--text);
  max-width: 1200px;
  margin: 0 auto;
  padding: 2em 1em;
  font-size: 14px;
  line-height: 1.6;
}

h1 {
  border-bottom: 1px solid var(--border);
  padding-bottom: 0.5em;
  font-size: 24px;
  font-weight: 650;
  letter-spacing: -0.01em;
  margin-bottom: 1em;
}

.card {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1.5em;
  margin: 1em 0;
}

.meta {
  color: var(--muted);
  font-size: 13px;
  margin-top: 0.5em;
}

a {
  color: var(--accent);
  text-decoration: none;
  transition: opacity 0.15s;
}

a:hover {
  opacity: 0.8;
}

code {
  background: var(--panel2);
  color: var(--text);
  padding: 0.2em 0.4em;
  border-radius: 3px;
  font-size: 13px;
  font-family: var(--mono);
}

a code {
  color: var(--accent);
}

.button {
  display: inline-block;
  background: var(--accent);
  color: var(--panel);
  padding: 0.5em 1em;
  border-radius: 4px;
  margin-right: 0.5em;
  font-weight: 600;
  transition: opacity 0.15s;
}

.button:hover {
  opacity: 0.9;
}

ul {
  padding-left: 1.5em;
  margin: 0.5em 0;
}

li {
  margin: 0.3em 0;
}
  </style>
  <script src="/nav.js" defer></script>
</head>
<body>
  <h1>facet performance benchmarks</h1>
  <p>Automated benchmark results published from CI. <a href="branches.html">View all branches →</a></p>
EOF

if [[ -n "${branches[main]:-}" ]]; then
  latest_commit=$(echo "${branches[main]}" | head -1 | cut -d: -f1 || echo "")
  if [[ -n "$latest_commit" ]]; then
    echo "  <div class='card'>" >> index.html
    echo "    <h2>Latest: <code>$latest_commit</code></h2>" >> index.html
    echo "    <div>" >> index.html
    echo "      <a class='button' href='main/$latest_commit/report-deser.html'>Deserialization →</a>" >> index.html
    echo "      <a class='button' href='main/$latest_commit/report-ser.html'>Serialization →</a>" >> index.html
    echo "    </div>" >> index.html
    echo "    <div class='meta'>Branch: main</div>" >> index.html
    echo "  </div>" >> index.html
  fi
fi

# Add "Recent branches" section showing branches with activity in last 7 days
NOW=$(date +%s)
SEVEN_DAYS=$((7 * 24 * 60 * 60))

# Collect recent branches (excluding main, which we already showed)
declare -a recent_branches_list
for branch in "${!branches[@]}"; do
  [[ "$branch" == "main" ]] && continue

  # Get the timestamp of the latest commit in this branch
  latest_commit_entry=$(echo "${branches[$branch]}" | head -1)
  latest_ts=$(echo "$latest_commit_entry" | cut -d: -f2)

  # Check if it's within the last 7 days
  age=$((NOW - latest_ts))
  if [[ $age -le $SEVEN_DAYS ]]; then
    # Store as "timestamp:branch" for sorting
    recent_branches_list+=("$latest_ts:$branch")
  fi
done

# Sort recent branches by timestamp (newest first)
if [[ ${#recent_branches_list[@]} -gt 0 ]]; then
  IFS=$'\n' sorted_recent=($(sort -t: -k1 -rn <<<"${recent_branches_list[*]}" || true))

  echo "  <div class='card'>" >> index.html
  echo "    <h2>Recent Activity</h2>" >> index.html
  echo "    <p style='color: var(--muted); margin-bottom: 1em;'>Branches with commits in the last 7 days</p>" >> index.html

  # Show up to 5 recent branches
  count=0
  for branch_entry in "${sorted_recent[@]}"; do
    [[ $count -ge 5 ]] && break
    ((count++))

    branch=$(echo "$branch_entry" | cut -d: -f2)

    # Get the 1-2 most recent commits from this branch
    IFS=$'\n' branch_commits=(${branches[$branch]})
    commits_to_show=2
    shown=0

    echo "    <div style='margin: 1em 0; padding: 1em; background: var(--panel2); border-radius: 6px;'>" >> index.html
    echo "      <h3 style='margin-bottom: 0.5em; font-size: 15px;'>$branch</h3>" >> index.html
    echo "      <ul style='list-style: none; padding: 0;'>" >> index.html

    for commit_entry in "${branch_commits[@]}"; do
      [[ $shown -ge $commits_to_show ]] && break
      ((shown++))

      commit=$(echo "$commit_entry" | cut -d: -f1)
      [[ -z "$commit" ]] && continue

      # Extract metadata if available
      metadata="${commit_metadata[$branch/$commit]:-}"
      if [[ -n "$metadata" ]]; then
        commit_short=$(echo "$metadata" | cut -d'|' -f4)
        timestamp_display=$(echo "$metadata" | cut -d'|' -f3)
      else
        commit_short="${commit:0:7}"
        timestamp_display=""
      fi

      echo "        <li style='margin: 0.5em 0;'>" >> index.html
      echo "          <a href='$branch/$commit/report-deser.html'><code>$commit_short</code></a>" >> index.html
      if [[ -n "$timestamp_display" ]]; then
        echo "          <span style='color: var(--muted); margin-left: 0.5em;'>$timestamp_display</span>" >> index.html
      fi
      echo "          <span style='margin-left: 0.5em;'>" >> index.html
      echo "            <a href='$branch/$commit/report-deser.html'>deser</a> | " >> index.html
      echo "            <a href='$branch/$commit/report-ser.html'>ser</a>" >> index.html
      echo "          </span>" >> index.html
      echo "        </li>" >> index.html
    done

    echo "      </ul>" >> index.html
    echo "    </div>" >> index.html
  done

  echo "  </div>" >> index.html
fi

cat >> index.html <<'EOF'
  <div class='card'>
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
</body>
</html>
EOF

# Generate branches.html (all branches and commits)
cat > branches.html <<'EOF'
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>facet benchmarks - all branches</title>
  <link rel="icon" href="/favicon.png" sizes="32x32" type="image/png">
  <link rel="icon" href="/favicon.ico" type="image/x-icon">
  <link rel="apple-touch-icon" href="/favicon.png">
  <style>
@font-face {
  font-family: 'Iosevka FTL';
  src: url('fonts/IosevkaFtl-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'Iosevka FTL';
  src: url('fonts/IosevkaFtl-Bold.ttf') format('truetype');
  font-weight: 600 700;
  font-style: normal;
  font-display: swap;
}

:root {
  color-scheme: light dark;

  --mono: 'Iosevka FTL', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;

  /* Surfaces */
  --bg:     light-dark(#fbfbfc, #0b0e14);
  --panel:  light-dark(#ffffff, #0f1420);
  --panel2: light-dark(#f6f7f9, #0c111b);

  /* Text */
  --text:  light-dark(#0e1116, #e7eaf0);
  --muted: light-dark(#3a4556, #a3adbd);

  /* Borders */
  --border:  light-dark(rgba(0,0,0,0.1), rgba(255,255,255,0.1));

  /* Semantic */
  --accent: light-dark(#2457f5, #7aa2f7);
}

* { margin: 0; padding: 0; box-sizing: border-box; }

body {
  font-family: var(--mono);
  background: var(--bg);
  color: var(--text);
  max-width: 1400px;
  margin: 0 auto;
  padding: 2em 1em;
  font-size: 14px;
  line-height: 1.6;
}

h1, h2 {
  border-bottom: 1px solid var(--border);
  padding-bottom: 0.5em;
  font-weight: 650;
  letter-spacing: -0.01em;
  margin-bottom: 0.8em;
}

h1 { font-size: 24px; }
h2 { font-size: 18px; margin-top: 2em; }

table {
  width: 100%;
  border-collapse: collapse;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
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

tr:last-child td {
  border-bottom: none;
}

a {
  color: var(--accent);
  text-decoration: none;
  transition: opacity 0.15s;
}

a:hover {
  opacity: 0.8;
}

code {
  background: var(--panel2);
  color: var(--text);
  padding: 0.2em 0.4em;
  border-radius: 3px;
  font-size: 13px;
  font-family: var(--mono);
}

a code {
  color: var(--accent);
}

.branch-section {
  background: var(--panel);
  margin: 1em 0;
  padding: 1em;
  border-radius: 8px;
  border: 1px solid var(--border);
}
  </style>
  <script src="/nav.js" defer></script>
</head>
<body>
  <h1>facet benchmarks - all branches</h1>
  <p><a href="index.html">← Back to latest main</a></p>
EOF

# Helper function to generate a branch section
generate_branch_section() {
  local branch="$1"
  local max_commits="${2:-10}"  # Default to 10 commits
  local is_stale="${3:-false}"

  IFS=$'\n' branch_commits=(${branches[$branch]})
  local total_commits=${#branch_commits[@]}

  echo "  <div class='branch-section'>" >> branches.html

  # Add branch header with commit count
  if [[ "$is_stale" == "true" ]]; then
    echo "    <h2>$branch <span style='color: var(--muted); font-size: 14px; font-weight: 400;'>(stale, $total_commits commits)</span></h2>" >> branches.html
  else
    echo "    <h2>$branch <span style='color: var(--muted); font-size: 14px; font-weight: 400;'>($total_commits commits)</span></h2>" >> branches.html
  fi

  echo "    <table>" >> branches.html
  echo "      <tr>" >> branches.html
  echo "        <th>Commit</th>" >> branches.html
  echo "        <th>Branch</th>" >> branches.html
  echo "        <th>PR</th>" >> branches.html
  echo "        <th>Generated</th>" >> branches.html
  echo "        <th>Reports</th>" >> branches.html
  echo "      </tr>" >> branches.html

  # Show only the most recent N commits
  local shown=0
  for commit_entry in "${branch_commits[@]}"; do
    [[ $shown -ge $max_commits ]] && break
    ((shown++))

    commit=$(echo "$commit_entry" | cut -d: -f1)
    [[ -z "$commit" ]] && continue

    # Extract metadata if available
    metadata="${commit_metadata[$branch/$commit]:-}"
    if [[ -n "$metadata" ]]; then
      branch_orig=$(echo "$metadata" | cut -d'|' -f1)
      pr_number=$(echo "$metadata" | cut -d'|' -f2)
      timestamp_display=$(echo "$metadata" | cut -d'|' -f3)
      commit_short=$(echo "$metadata" | cut -d'|' -f4)
    else
      branch_orig="$branch"
      pr_number=""
      timestamp_display=""
      commit_short="${commit:0:7}"
    fi

    echo "      <tr>" >> branches.html
    echo "        <td><a href='https://github.com/facet-rs/facet/commit/$commit'><code>$commit_short</code></a></td>" >> branches.html
    echo "        <td><a href='https://github.com/facet-rs/facet/tree/$branch_orig'>$branch_orig</a></td>" >> branches.html
    if [[ -n "$pr_number" ]]; then
      echo "        <td><a href='https://github.com/facet-rs/facet/pull/$pr_number'>#$pr_number</a></td>" >> branches.html
    else
      echo "        <td>—</td>" >> branches.html
    fi
    if [[ -n "$timestamp_display" ]]; then
      echo "        <td>$timestamp_display</td>" >> branches.html
    else
      echo "        <td>—</td>" >> branches.html
    fi
    echo "        <td>" >> branches.html
    echo "          <a href='$branch/$commit/report-deser.html'>deserialize</a> | " >> branches.html
    echo "          <a href='$branch/$commit/report-ser.html'>serialize</a>" >> branches.html
    echo "        </td>" >> branches.html
    echo "      </tr>" >> branches.html
  done

  echo "    </table>" >> branches.html
  echo "  </div>" >> branches.html
}

# Constants for staleness check
NINETY_DAYS=$((90 * 24 * 60 * 60))

# 1. Show main branch first (if it exists)
if [[ -n "${branches[main]:-}" ]]; then
  generate_branch_section "main" 10 false
fi

# 2. Collect other branches with their latest timestamp
declare -a active_branches
declare -a stale_branches

for branch in "${!branches[@]}"; do
  [[ "$branch" == "main" ]] && continue

  # Get the timestamp of the latest commit in this branch
  latest_commit_entry=$(echo "${branches[$branch]}" | head -1)
  latest_ts=$(echo "$latest_commit_entry" | cut -d: -f2)

  # Check if stale (>90 days old)
  age=$((NOW - latest_ts))
  if [[ $age -gt $NINETY_DAYS ]]; then
    stale_branches+=("$latest_ts:$branch")
  else
    active_branches+=("$latest_ts:$branch")
  fi
done

# 3. Sort active branches by timestamp (newest first) and display
if [[ ${#active_branches[@]} -gt 0 ]]; then
  IFS=$'\n' sorted_active=($(sort -t: -k1 -rn <<<"${active_branches[*]}" || true))
  for branch_entry in "${sorted_active[@]}"; do
    branch=$(echo "$branch_entry" | cut -d: -f2)
    generate_branch_section "$branch" 10 false
  done
fi

# 4. Show stale branches in a collapsible section
if [[ ${#stale_branches[@]} -gt 0 ]]; then
  IFS=$'\n' sorted_stale=($(sort -t: -k1 -rn <<<"${stale_branches[*]}" || true))

  echo "  <details style='margin-top: 2em;'>" >> branches.html
  echo "    <summary style='cursor: pointer; padding: 1em; background: var(--panel); border: 1px solid var(--border); border-radius: 8px; font-weight: 600;'>" >> branches.html
  echo "      Stale branches (no commits in last 90 days) — ${#stale_branches[@]} branches" >> branches.html
  echo "    </summary>" >> branches.html

  for branch_entry in "${sorted_stale[@]}"; do
    branch=$(echo "$branch_entry" | cut -d: -f2)
    generate_branch_section "$branch" 10 true
  done

  echo "  </details>" >> branches.html
fi

cat >> branches.html <<'EOF'
</body>
</html>
EOF

# Generate index.json for navigation dropdowns
echo "Generating index.json for navigation..."

echo "{" > index.json
echo '  "branches": {' >> index.json

first_branch=true
for branch in "${!branches[@]}"; do
  # Add comma for all but first entry
  if [[ "$first_branch" == "true" ]]; then
    first_branch=false
  else
    echo "," >> index.json
  fi

  echo -n "    \"$branch\": [" >> index.json

  IFS=$'\n' branch_commits=(${branches[$branch]})
  first_commit=true
  for commit_entry in "${branch_commits[@]}"; do
    commit=$(echo "$commit_entry" | cut -d: -f1)
    [[ -z "$commit" ]] && continue

    # Add comma for all but first commit
    if [[ "$first_commit" == "true" ]]; then
      first_commit=false
    else
      echo -n "," >> index.json
    fi

    # Extract metadata if available
    metadata="${commit_metadata[$branch/$commit]:-}"
    if [[ -n "$metadata" ]]; then
      branch_orig=$(echo "$metadata" | cut -d'|' -f1)
      pr_number=$(echo "$metadata" | cut -d'|' -f2)
      timestamp_display=$(echo "$metadata" | cut -d'|' -f3)
      commit_short=$(echo "$metadata" | cut -d'|' -f4)
    else
      branch_orig="$branch"
      pr_number=""
      timestamp_display=""
      commit_short="${commit:0:7}"
    fi

    echo >> index.json
    echo -n "      {" >> index.json
    echo -n "\"commit\":\"$commit\"," >> index.json
    echo -n "\"commit_short\":\"$commit_short\"," >> index.json
    echo -n "\"branch_original\":\"$branch_orig\"," >> index.json
    if [[ -n "$pr_number" ]]; then
      echo -n "\"pr_number\":\"$pr_number\"," >> index.json
    fi
    if [[ -n "$timestamp_display" ]]; then
      echo -n "\"timestamp_display\":\"$timestamp_display\"" >> index.json
    else
      echo -n "\"timestamp_display\":\"\"" >> index.json
    fi
    echo -n "}" >> index.json
  done

  echo >> index.json
  echo -n "    ]" >> index.json
done

echo >> index.json
echo "  }" >> index.json
echo "}" >> index.json

echo "✅ Generated index.html, branches.html, and index.json"
