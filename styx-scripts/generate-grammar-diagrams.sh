#!/usr/bin/env bash
set -euo pipefail

# Generate railroad diagrams from EBNF grammar
# Requires: java, rr.war (railroad diagram generator)

cd "$(dirname "$0")/../docs"

rm -rf static/grammar
java -jar ~/.cargo/bin/rr.war -md -noembedded -out:grammar-output.zip grammar.ebnf
unzip -o grammar-output.zip
mkdir -p static/grammar
mv diagram/*.svg static/grammar/
rmdir diagram
rm -f static/grammar/rr-2.6.svg

# Add frontmatter and fix paths
cat > content/spec/grammar.md << 'EOF'
+++
title = "Grammar"
weight = 3
slug = "grammar"
insert_anchor_links = "heading"
+++

Visual grammar reference for STYX. See [Parser](@/spec/parser.md) for normative rules.

EOF

sed 's|diagram/|/grammar/|g' index.md >> content/spec/grammar.md
rm index.md grammar-output.zip
