#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

function parseCsvInts(value, flag) {
  const out = value
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => {
      const n = Number.parseInt(s, 10);
      if (!Number.isFinite(n) || n <= 0) {
        throw new Error(`invalid ${flag} value: ${s}`);
      }
      return n;
    });
  if (out.length === 0) {
    throw new Error(`no values provided for ${flag}`);
  }
  return out;
}

function parseCsvStrings(value, flag) {
  const out = value.split(',').map((s) => s.trim()).filter(Boolean);
  if (out.length === 0) {
    throw new Error(`no values provided for ${flag}`);
  }
  return out;
}

function shuffle(xs) {
  const out = xs.slice();
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

function usage(code = 0) {
  const msg = [
    'usage: node scripts/run_bench_blocks.mjs [options]',
    '',
    'options:',
    '  --workload <echo|canvas|gnarly>',
    '  --payload-sizes <csv>',
    '  --in-flights <csv>',
    '  --blocks <n>',
    '  --warmup-secs <n>',
    '  --measure-secs <n>',
    '  --count <n>',
    '  --transports <local,shm>',
    '  --out <path>',
    '  --logs-dir <dir>',
  ].join('\n');
  (code === 0 ? console.log : console.error)(msg);
  process.exit(code);
}

function parseArgs(argv) {
  const out = {
    workload: 'gnarly',
    payloadSizes: [1, 4, 16, 64, 256, 1024],
    inFlights: [1, 16, 64],
    blocks: 5,
    warmupSecs: 2,
    measureSecs: 5,
    count: null,
    transports: ['local', 'shm'],
    out: '/tmp/bench-blocks.json',
    logsDir: '/tmp/bench-blocks-logs',
  };

  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--workload':
        out.workload = argv[++i];
        break;
      case '--payload-sizes':
        out.payloadSizes = parseCsvInts(argv[++i], '--payload-sizes');
        break;
      case '--in-flights':
        out.inFlights = parseCsvInts(argv[++i], '--in-flights');
        break;
      case '--blocks':
        out.blocks = Number.parseInt(argv[++i], 10);
        break;
      case '--warmup-secs':
        out.warmupSecs = Number.parseFloat(argv[++i]);
        break;
      case '--measure-secs':
        out.measureSecs = Number.parseFloat(argv[++i]);
        break;
      case '--count':
        out.count = Number.parseInt(argv[++i], 10);
        break;
      case '--transports':
        out.transports = parseCsvStrings(argv[++i], '--transports');
        break;
      case '--out':
        out.out = argv[++i];
        break;
      case '--logs-dir':
        out.logsDir = argv[++i];
        break;
      case '--help':
      case '-h':
        usage(0);
        break;
      default:
        console.error(`unknown arg: ${arg}`);
        usage(1);
    }
  }

  if (!Number.isInteger(out.blocks) || out.blocks <= 0) {
    throw new Error('--blocks must be > 0');
  }
  if (out.count == null) {
    if (!(out.warmupSecs >= 0) || !(out.measureSecs > 0)) {
      throw new Error('--warmup-secs must be >= 0 and --measure-secs must be > 0');
    }
  } else if (!Number.isInteger(out.count) || out.count <= 0) {
    throw new Error('--count must be > 0');
  }

  return out;
}

function addrForTransport(transport) {
  if (transport === 'local') return 'local:///tmp/bench.vox';
  if (transport === 'shm') return 'shm:///tmp/bench-shm.sock';
  throw new Error(`unsupported transport: ${transport}`);
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function parsePeakRssKib(stderr) {
  const match = stderr.match(/peak_rss_kib=(\d+)/);
  return match ? Number.parseInt(match[1], 10) : null;
}

function runTrial(args, blockIndex, orderInBlock, condition) {
  const addr = addrForTransport(condition.transport);
  const label = `b${blockIndex + 1}-o${orderInBlock + 1}-${condition.transport}-p${condition.payloadSize}-i${condition.inFlight}`;
  const trialArgs = [
    '--addr', addr,
    '--',
    '--workload', args.workload,
    '--addr', addr,
    '--payload-sizes', String(condition.payloadSize),
    '--in-flights', String(condition.inFlight),
    '--json',
  ];
  if (args.count != null) {
    trialArgs.push('--count', String(args.count));
  } else {
    trialArgs.push('--warmup-secs', String(args.warmupSecs));
    trialArgs.push('--measure-secs', String(args.measureSecs));
  }

  console.error(`trial ${label}`);
  const result = spawnSync('./target/release/examples/bench_runner', trialArgs, {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });

  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  fs.writeFileSync(path.join(args.logsDir, `${label}.stdout.json`), stdout);
  fs.writeFileSync(path.join(args.logsDir, `${label}.stderr.log`), stderr);

  if (result.status !== 0) {
    throw new Error(`trial ${label} failed with status ${result.status}\n${stderr}`);
  }

  const rows = JSON.parse(stdout);
  if (!Array.isArray(rows) || rows.length !== 1) {
    throw new Error(`trial ${label} produced ${rows.length ?? 'non-array'} rows, expected 1`);
  }

  return {
    ...rows[0],
    block: blockIndex + 1,
    order_in_block: orderInBlock + 1,
    peak_rss_kib: parsePeakRssKib(stderr),
    label,
  };
}

function build() {
  const result = spawnSync('cargo', ['build', '--quiet', '-p', 'rust-examples', '--example', 'bench_runner', '--example', 'bench_client', '--release'], {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`cargo build failed\n${result.stderr ?? ''}`);
  }
}

function main() {
  const args = parseArgs(process.argv);
  ensureDir(args.logsDir);
  build();

  const baseConditions = [];
  for (const transport of args.transports) {
    for (const payloadSize of args.payloadSizes) {
      for (const inFlight of args.inFlights) {
        baseConditions.push({ transport, payloadSize, inFlight });
      }
    }
  }

  const rows = [];
  for (let blockIndex = 0; blockIndex < args.blocks; blockIndex++) {
    const conditions = shuffle(baseConditions);
    for (let orderInBlock = 0; orderInBlock < conditions.length; orderInBlock++) {
      rows.push(runTrial(args, blockIndex, orderInBlock, conditions[orderInBlock]));
    }
  }

  fs.writeFileSync(args.out, `${JSON.stringify(rows, null, 2)}\n`);
  console.log(`wrote ${rows.length} trial rows to ${args.out}`);
}

main();
