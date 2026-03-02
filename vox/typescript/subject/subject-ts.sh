#!/bin/bash
set -eu

# Prefer Node already on PATH (e.g. via actions/setup-node in CI).
if command -v node >/dev/null 2>&1; then
  exec node --experimental-transform-types typescript/subject/subject.ts
fi

# Fallback for local environments that rely on nvm.
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
if [ -s "$NVM_DIR/nvm.sh" ]; then
  # shellcheck source=/dev/null
  . "$NVM_DIR/nvm.sh"
  nvm use 25 >/dev/null 2>&1 || nvm use >/dev/null 2>&1
  exec node --experimental-transform-types typescript/subject/subject.ts
fi

echo "subject-ts: node not found on PATH and nvm is unavailable" >&2
exit 127
