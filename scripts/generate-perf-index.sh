#!/usr/bin/env bash
# Generate index.html and branches.html for perf.facet.rs
set -euo pipefail

PERF_DIR="${1:?Usage: $0 <perf-directory>}"
cd "$PERF_DIR"

# Collect branches and commits
declare -A branches
for branch_dir in */; do
  branch="${branch_dir%/}"
  [[ "$branch" == "fonts" ]] && continue
  [[ ! -d "$branch_dir" ]] && continue

  commits=()
  for commit_dir in "$branch_dir"*/; do
    commit="${commit_dir%/}"
    commit="${commit##*/}"
    [[ "$commit" == "latest" ]] && continue

    timestamp=$(stat -c %Y "$commit_dir" 2>/dev/null || echo 0)
    commits+=("$commit:$timestamp")
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
    body { font-family: system-ui; max-width: 1200px; margin: 2em auto; padding: 0 1em; background: #fafafa; }
    h1 { border-bottom: 2px solid #333; padding-bottom: 0.5em; }
    .card { background: white; border: 1px solid #ddd; border-radius: 8px; padding: 1.5em; margin: 1em 0; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
    .meta { color: #666; font-size: 0.9em; margin-top: 0.5em; }
    a { color: #0066cc; text-decoration: none; }
    a:hover { text-decoration: underline; }
    .button { display: inline-block; background: #0066cc; color: white; padding: 0.5em 1em; border-radius: 4px; margin-right: 0.5em; }
    .button:hover { background: #0052a3; text-decoration: none; }
  </style>
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
    body { font-family: system-ui; max-width: 1400px; margin: 2em auto; padding: 0 1em; background: #fafafa; }
    h1, h2 { border-bottom: 2px solid #333; padding-bottom: 0.5em; }
    table { width: 100%; border-collapse: collapse; background: white; }
    th, td { text-align: left; padding: 0.75em; border-bottom: 1px solid #eee; }
    th { background: #f5f5f5; font-weight: 600; }
    a { color: #0066cc; text-decoration: none; }
    a:hover { text-decoration: underline; }
    code { background: #f5f5f5; padding: 0.2em 0.4em; border-radius: 3px; font-size: 0.9em; }
    .branch-section { background: white; margin: 1em 0; padding: 1em; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
  </style>
</head>
<body>
  <h1>facet benchmarks - all branches</h1>
  <p><a href="index.html">← Back to latest main</a></p>
EOF

for branch in $(echo "${!branches[@]}" | tr ' ' '\n' | sort); do
  echo "  <div class='branch-section'>" >> branches.html
  echo "    <h2>$branch</h2>" >> branches.html
  echo "    <table>" >> branches.html
  echo "      <tr><th>Commit</th><th>Reports</th></tr>" >> branches.html

  IFS=$'\n' commits=(${branches[$branch]})
  for commit_entry in "${commits[@]}"; do
    commit=$(echo "$commit_entry" | cut -d: -f1)
    [[ -z "$commit" ]] && continue
    echo "      <tr>" >> branches.html
    echo "        <td><code>$commit</code></td>" >> branches.html
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
