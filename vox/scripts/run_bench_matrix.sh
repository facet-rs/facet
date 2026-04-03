#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

workload="${1:-echo}"
payload_sizes="${PAYLOAD_SIZES:-16,128,1024,8192,65536,262144}"
in_flights="${IN_FLIGHTS:-1,64,256}"
count="${COUNT:-}"
warmup_secs="${WARMUP_SECS:-2}"
measure_secs="${MEASURE_SECS:-5}"

local_addr="${LOCAL_ADDR:-local:///tmp/bench.vox}"
shm_addr="${SHM_ADDR:-shm:///tmp/bench-shm.sock}"

local_json="${LOCAL_JSON:-/tmp/bench-local.json}"
shm_json="${SHM_JSON:-/tmp/bench-shm.json}"
local_log="${LOCAL_LOG:-/tmp/bench-local.log}"
shm_log="${SHM_LOG:-/tmp/bench-shm.log}"

cargo build --quiet -p rust-examples --example bench_runner --example bench_client --release

pkill -f 'samply load -P 3000 --no-open /Users/amos/bearcove/vox/profile.swift-local.json.gz' >/dev/null 2>&1 || true
pkill -f './target/release/examples/bench_runner --addr shm:///tmp/bench-shm.sock' >/dev/null 2>&1 || true
pkill -f '/Users/amos/bearcove/vox/target/release/examples/bench_client --addr shm:///tmp/bench-shm.sock' >/dev/null 2>&1 || true

rm -f "$local_json" "$shm_json" "$local_log" "$shm_log"
rm -f /tmp/bench.vox /tmp/bench.vox.lock /tmp/bench-shm.sock /tmp/bench-shm.sock.lock

client_args=(
  --workload "$workload"
  --payload-sizes "$payload_sizes"
  --in-flights "$in_flights"
  --json
)

if [[ -n "$count" ]]; then
  client_args+=(--count "$count")
else
  client_args+=(--warmup-secs "$warmup_secs" --measure-secs "$measure_secs")
fi

./target/release/examples/bench_runner \
  --addr "$local_addr" \
  -- \
  --addr "$local_addr" \
  "${client_args[@]}" \
  >"$local_json" 2>"$local_log"

./target/release/examples/bench_runner \
  --addr "$shm_addr" \
  -- \
  --addr "$shm_addr" \
  "${client_args[@]}" \
  >"$shm_json" 2>"$shm_log"

node rust-examples/bench_matrix_report.js --local "$local_json" --shm "$shm_json"
