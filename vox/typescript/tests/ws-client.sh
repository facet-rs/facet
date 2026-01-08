#!/bin/bash
# WebSocket client wrapper for cross-language testing
set -eu
cd "$(dirname "$0")/../.."
source ~/.nvm/nvm.sh
nvm use 25 >/dev/null 2>&1
exec node --experimental-transform-types typescript/tests/ws-client.ts
