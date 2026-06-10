#!/usr/bin/env node

import fs from "node:fs";

function parseArgs(argv) {
  const out = {
    local: null,
    ffi: null,
  };

  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--local") {
      out.local = argv[++i];
      continue;
    }
    if (arg === "--ffi") {
      out.ffi = argv[++i];
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      printUsage(0);
    }
    console.error(`unknown arg: ${arg}`);
    printUsage(1);
  }

  if (!out.local || !out.ffi) {
    printUsage(1);
  }
  return out;
}

function printUsage(code) {
  const msg =
    "usage: node rust-examples/bench_matrix_report.js --local /tmp/bench-local.json --ffi /tmp/bench-ffi.json";
  if (code === 0) {
    console.log(msg);
  } else {
    console.error(msg);
  }
  process.exit(code);
}

function loadJson(path) {
  const raw = fs.readFileSync(path, "utf8");
  const parsed = JSON.parse(raw);
  if (!Array.isArray(parsed)) {
    throw new Error(`${path}: expected top-level array`);
  }
  return parsed;
}

function keyOf(row) {
  return `${row.payload_size}|${row.in_flight}`;
}

function num(v) {
  return typeof v === "number" && Number.isFinite(v) ? v : NaN;
}

function fmtNum(v, digits = 3) {
  return Number.isFinite(v) ? v.toFixed(digits) : "-";
}

function mean(xs) {
  return xs.reduce((a, b) => a + b, 0) / xs.length;
}

function geomean(xs) {
  return Math.exp(xs.reduce((a, b) => a + Math.log(b), 0) / xs.length);
}

function main() {
  const args = parseArgs(process.argv);
  const localRows = loadJson(args.local);
  const ffiRows = loadJson(args.ffi);

  const localMap = new Map(localRows.map((r) => [keyOf(r), r]));
  const ffiMap = new Map(ffiRows.map((r) => [keyOf(r), r]));

  const keys = [...new Set([...localMap.keys(), ...ffiMap.keys()])].sort((a, b) => {
    const [pa, ia] = a.split("|").map(Number);
    const [pb, ib] = b.split("|").map(Number);
    return pa - pb || ia - ib;
  });

  console.log(`local rows=${localRows.length} ffi rows=${ffiRows.length}`);
  console.log("payload\tin_flight\tlocal_us\tffi_us\tffi_vs_local");

  const overlapRatios = [];
  for (const k of keys) {
    const l = localMap.get(k);
    const s = ffiMap.get(k);
    const [payload, inFlight] = k.split("|");
    const lUs = num(l?.per_call_micros);
    const sUs = num(s?.per_call_micros);
    const ratio = Number.isFinite(lUs) && Number.isFinite(sUs) ? sUs / lUs : NaN;
    const deltaPct = Number.isFinite(ratio) ? ((ratio - 1) * 100).toFixed(1) + "%" : "n/a";
    if (Number.isFinite(ratio)) {
      overlapRatios.push(ratio);
    }
    console.log(
      `${payload}\t${inFlight}\t${fmtNum(lUs)}\t${fmtNum(sUs)}\t${deltaPct}`
    );
  }

  if (overlapRatios.length === 0) {
    console.log("overlap=0");
    return;
  }

  console.log(
    `overlap=${overlapRatios.length} avg_ratio=${mean(overlapRatios).toFixed(3)} geomean_ratio=${geomean(overlapRatios).toFixed(3)}`
  );
}

main();
