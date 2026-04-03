#!/usr/bin/env node

import fs from 'node:fs';

function usage(code = 0) {
  const msg = 'usage: node rust-examples/bench_blocks_report.js --input /tmp/bench-blocks.json';
  (code === 0 ? console.log : console.error)(msg);
  process.exit(code);
}

function parseArgs(argv) {
  const out = { input: null, samples: 5000 };
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--input') {
      out.input = argv[++i];
    } else if (arg === '--samples') {
      out.samples = Number.parseInt(argv[++i], 10);
    } else if (arg === '--help' || arg === '-h') {
      usage(0);
    } else {
      console.error(`unknown arg: ${arg}`);
      usage(1);
    }
  }
  if (!out.input) usage(1);
  return out;
}

function mean(xs) {
  return xs.reduce((a, b) => a + b, 0) / xs.length;
}

function percentile(sorted, q) {
  if (sorted.length === 0) return NaN;
  const idx = (sorted.length - 1) * q;
  const lo = Math.floor(idx);
  const hi = Math.ceil(idx);
  if (lo === hi) return sorted[lo];
  const frac = idx - lo;
  return sorted[lo] * (1 - frac) + sorted[hi] * frac;
}

function bootstrapMeanCi(values, samples) {
  const resampledMeans = [];
  for (let i = 0; i < samples; i++) {
    const picked = [];
    for (let j = 0; j < values.length; j++) {
      picked.push(values[Math.floor(Math.random() * values.length)]);
    }
    resampledMeans.push(mean(picked));
  }
  resampledMeans.sort((a, b) => a - b);
  return {
    lo: percentile(resampledMeans, 0.025),
    hi: percentile(resampledMeans, 0.975),
  };
}

function keyOf(row) {
  return `${row.payload_size}|${row.in_flight}`;
}

function fmt(v, digits = 2) {
  return Number.isFinite(v) ? v.toFixed(digits) : '-';
}

function pctDelta(shm, local) {
  return ((shm / local) - 1) * 100;
}

function main() {
  const args = parseArgs(process.argv);
  const rows = JSON.parse(fs.readFileSync(args.input, 'utf8'));
  const byCondition = new Map();

  for (const row of rows) {
    const key = keyOf(row);
    if (!byCondition.has(key)) byCondition.set(key, []);
    byCondition.get(key).push(row);
  }

  console.log('payload\tin_flight\tblocks\tp50_delta_pct\tp50_ci\tp99_delta_pct\tp99_ci\tthroughput_delta_pct\tthroughput_ci\trss_delta_pct\trss_ci');
  for (const [key, conditionRows] of [...byCondition.entries()].sort((a, b) => {
    const [ap, ai] = a[0].split('|').map(Number);
    const [bp, bi] = b[0].split('|').map(Number);
    return ap - bp || ai - bi;
  })) {
    const perBlock = new Map();
    for (const row of conditionRows) {
      if (!perBlock.has(row.block)) perBlock.set(row.block, {});
      perBlock.get(row.block)[row.transport] = row;
    }

    const p50Deltas = [];
    const p99Deltas = [];
    const throughputDeltas = [];
    const rssDeltas = [];
    for (const pair of perBlock.values()) {
      if (!pair.local || !pair.shm) continue;
      p50Deltas.push(pctDelta(pair.shm.p50_us, pair.local.p50_us));
      p99Deltas.push(pctDelta(pair.shm.p99_us, pair.local.p99_us));
      throughputDeltas.push(pctDelta(pair.shm.calls_per_sec, pair.local.calls_per_sec));
      if (pair.local.peak_rss_kib && pair.shm.peak_rss_kib) {
        rssDeltas.push(pctDelta(pair.shm.peak_rss_kib, pair.local.peak_rss_kib));
      }
    }

    const [payload, inFlight] = key.split('|');
    const p50Ci = p50Deltas.length ? bootstrapMeanCi(p50Deltas, args.samples) : { lo: NaN, hi: NaN };
    const p99Ci = p99Deltas.length ? bootstrapMeanCi(p99Deltas, args.samples) : { lo: NaN, hi: NaN };
    const throughputCi = throughputDeltas.length ? bootstrapMeanCi(throughputDeltas, args.samples) : { lo: NaN, hi: NaN };
    const rssCi = rssDeltas.length ? bootstrapMeanCi(rssDeltas, args.samples) : { lo: NaN, hi: NaN };

    console.log([
      payload,
      inFlight,
      p50Deltas.length,
      fmt(mean(p50Deltas)),
      `[${fmt(p50Ci.lo)},${fmt(p50Ci.hi)}]`,
      fmt(mean(p99Deltas)),
      `[${fmt(p99Ci.lo)},${fmt(p99Ci.hi)}]`,
      fmt(mean(throughputDeltas)),
      `[${fmt(throughputCi.lo)},${fmt(throughputCi.hi)}]`,
      fmt(mean(rssDeltas)),
      `[${fmt(rssCi.lo)},${fmt(rssCi.hi)}]`,
    ].join('\t'));
  }
}

main();
