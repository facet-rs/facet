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

for branch in $(echo "${!branches[@]}" | tr ' ' '\n' | sort); do
  echo "  <div class='branch-section'>" >> branches.html
  echo "    <h2>$branch</h2>" >> branches.html
  echo "    <table>" >> branches.html
  echo "      <tr>" >> branches.html
  echo "        <th>Commit</th>" >> branches.html
  echo "        <th>Branch</th>" >> branches.html
  echo "        <th>PR</th>" >> branches.html
  echo "        <th>Generated</th>" >> branches.html
  echo "        <th>Reports</th>" >> branches.html
  echo "      </tr>" >> branches.html

  IFS=$'\n' commits=(${branches[$branch]})
  for commit_entry in "${commits[@]}"; do
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
done

cat >> branches.html <<'EOF'
</body>
</html>
EOF

echo "✅ Generated index.html and branches.html"
