#!/bin/bash
set -eu

# Load nvm
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"

nvm use 25 >/dev/null 2>&1

exec node --experimental-transform-types typescript/tests/tcp-client.ts
