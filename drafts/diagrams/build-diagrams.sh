#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Build individual SVGs
for f in *.dot; do
    out="${f%.dot}.svg"
    echo "Rendering $f -> $out"
    dot -Tsvg "$f" -o "$out"
done

# Now inline SVGs into the HTML
echo "Inlining SVGs into HTML..."

# Read template and replace <object> tags with inline SVGs
cp extension-attributes-design-v3.html extension-attributes-design-v3.html.bak

python3 - <<'PYTHON'
import re
from pathlib import Path

html = Path('extension-attributes-design-v3.html.bak').read_text()

# Map of graph titles to SVG files
svg_mappings = {
    'crate_responsibilities': '00-crate-responsibilities.svg',
    'dependency_graph': '00-dependency-graph.svg',
    'grammar_definition': '01-grammar-definition.svg',
    'attribute_usage': '02-attribute-usage.svg',
    'dispatch_detail': '02b-dispatch-detail.svg',
    'error_flow': '03-error-flow.svg',
    'storage_model': '04-storage-model.svg',
}

def load_svg(svg_file):
    svg_path = Path(svg_file)
    if svg_path.exists():
        svg_content = svg_path.read_text()
        # Remove XML declaration and DOCTYPE
        svg_content = re.sub(r'<\?xml[^?]*\?>\s*', '', svg_content)
        svg_content = re.sub(r'<!DOCTYPE[^>]*>\s*', '', svg_content)
        # Remove comments
        svg_content = re.sub(r'<!--[^>]*-->\s*', '', svg_content)
        return svg_content.strip()
    return None

def replace_object(match):
    svg_file = match.group(1)
    svg_content = load_svg(svg_file)
    if svg_content:
        return svg_content
    return match.group(0)

def replace_inline_svg(match):
    graph_title = match.group(1)
    if graph_title in svg_mappings:
        svg_content = load_svg(svg_mappings[graph_title])
        if svg_content:
            return svg_content
    return match.group(0)

# Replace <object type="image/svg+xml" data="foo.svg"></object> with inline SVG
html = re.sub(
    r'<object type="image/svg\+xml" data="([^"]+\.svg)"></object>',
    replace_object,
    html
)

# Also replace already-inlined SVGs by matching <title>name</title>
# Match: <svg ...>...<title>name</title>...</svg>
html = re.sub(
    r'<svg[^>]*>.*?<title>(\w+)</title>.*?</svg>',
    replace_inline_svg,
    html,
    flags=re.DOTALL
)

Path('extension-attributes-design-v3.html').write_text(html)
print("Done!")
PYTHON

rm extension-attributes-design-v3.html.bak
echo "Done!"
