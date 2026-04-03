#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

function usage(code = 0) {
  const msg = [
    'usage: node scripts/run_bench_open_loop_blocks.mjs [options]',
    '',
    'options:',
    '  --workload <echo|canvas|gnarly>',
    '  --payload-sizes <csv>',
    '  --in-flights <csv>',
    '  --blocks <n>',
    '  --warmup-secs <n>',
    '  --measure-secs <n>',
    '  --calibration-warmup-secs <n>',
    '  --calibration-measure-secs <n>',
    '  --calibration-target-drop-min <n>',
    '  --calibration-target-drop-max <n>',
    '  --calibration-max-probes <n>',
    '  --calibration-refine-steps <n>',
    '  --load-factors <csv>',
    '  --transports <local,shm>',
    '  --server-impls <swift,rust>',
    '  --out <path>',
    '  --logs-dir <dir>',
  ].join('\n');
  (code === 0 ? console.log : console.error)(msg);
  process.exit(code);
}

function parseCsvInts(value, flag) {
  const out = value.split(',').map((s) => s.trim()).filter(Boolean).map((s) => {
    const n = Number.parseInt(s, 10);
    if (!Number.isFinite(n) || n <= 0) throw new Error(`invalid ${flag} value: ${s}`);
    return n;
  });
  if (!out.length) throw new Error(`no values provided for ${flag}`);
  return out;
}

function parseCsvFloats(value, flag) {
  const out = value.split(',').map((s) => s.trim()).filter(Boolean).map((s) => {
    const n = Number.parseFloat(s);
    if (!Number.isFinite(n) || n <= 0) throw new Error(`invalid ${flag} value: ${s}`);
    return n;
  });
  if (!out.length) throw new Error(`no values provided for ${flag}`);
  return out;
}

function parseCsvStrings(value, flag) {
  const out = value.split(',').map((s) => s.trim()).filter(Boolean);
  if (!out.length) throw new Error(`no values provided for ${flag}`);
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

function parseArgs(argv) {
  const out = {
    workload: 'gnarly',
    payloadSizes: [1, 4, 16, 64, 256, 1024],
    inFlights: [1, 16, 64],
    blocks: 5,
    warmupSecs: 2,
    measureSecs: 5,
    calibrationWarmupSecs: 0.5,
    calibrationMeasureSecs: 1,
    loadFactors: [0.25, 0.5, 0.75, 0.9, 1.0, 1.1],
    calibrationStartRps: 100,
    calibrationMaxRps: 200000,
    calibrationTargetDropMin: 0.01,
    calibrationTargetDropMax: 0.05,
    calibrationMaxProbes: 8,
    calibrationRefineSteps: 4,
    transports: ['local', 'shm'],
    serverImpls: ['swift'],
    out: '/tmp/open-loop-blocks.json',
    logsDir: '/tmp/open-loop-blocks-logs',
    aggregateOnly: false,
  };

  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--workload': out.workload = argv[++i]; break;
      case '--payload-sizes': out.payloadSizes = parseCsvInts(argv[++i], '--payload-sizes'); break;
      case '--in-flights': out.inFlights = parseCsvInts(argv[++i], '--in-flights'); break;
      case '--blocks': out.blocks = Number.parseInt(argv[++i], 10); break;
      case '--warmup-secs': out.warmupSecs = Number.parseFloat(argv[++i]); break;
      case '--measure-secs': out.measureSecs = Number.parseFloat(argv[++i]); break;
      case '--calibration-warmup-secs': out.calibrationWarmupSecs = Number.parseFloat(argv[++i]); break;
      case '--calibration-measure-secs': out.calibrationMeasureSecs = Number.parseFloat(argv[++i]); break;
      case '--load-factors': out.loadFactors = parseCsvFloats(argv[++i], '--load-factors'); break;
      case '--calibration-start-rps': out.calibrationStartRps = Number.parseInt(argv[++i], 10); break;
      case '--calibration-max-rps': out.calibrationMaxRps = Number.parseInt(argv[++i], 10); break;
      case '--calibration-target-drop-min': out.calibrationTargetDropMin = Number.parseFloat(argv[++i]); break;
      case '--calibration-target-drop-max': out.calibrationTargetDropMax = Number.parseFloat(argv[++i]); break;
      case '--calibration-max-probes': out.calibrationMaxProbes = Number.parseInt(argv[++i], 10); break;
      case '--calibration-refine-steps': out.calibrationRefineSteps = Number.parseInt(argv[++i], 10); break;
      case '--transports': out.transports = parseCsvStrings(argv[++i], '--transports'); break;
      case '--server-impls': out.serverImpls = parseCsvStrings(argv[++i], '--server-impls'); break;
      case '--out': out.out = argv[++i]; break;
      case '--logs-dir': out.logsDir = argv[++i]; break;
      case '--aggregate-only': out.aggregateOnly = true; break;
      case '--help':
      case '-h': usage(0); break;
      default:
        console.error(`unknown arg: ${arg}`);
        usage(1);
    }
  }

  return out;
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function addrForTransport(transport) {
  if (transport === 'local') return 'local:///tmp/bench.vox';
  if (transport === 'shm') return 'shm:///tmp/bench-shm.sock';
  throw new Error(`unsupported transport: ${transport}`);
}

function parsePeakRssKib(stderr) {
  const match = stderr.match(/peak_rss_kib=(\d+)/);
  return match ? Number.parseInt(match[1], 10) : null;
}


function subjectCmdForServerImpl(serverImpl) {
  if (serverImpl === 'swift') return path.join(repoRoot, 'swift', 'subject', 'subject-swift.sh');
  if (serverImpl === 'rust') return path.join(repoRoot, 'target', 'release', 'subject-rust');
  throw new Error(`unsupported server impl: ${serverImpl}`);
}

function subjectModeFor(serverImpl, transport) {
  return 'server';
}

function supportsPair(serverImpl, transport) {
  return true;
}

function build() {
  const result = spawnSync('cargo', ['build', '--quiet', '-p', 'rust-examples', '--example', 'bench_runner', '--example', 'bench_client', '-p', 'subject-rust', '--release'], {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`cargo build failed\n${result.stderr ?? ''}`);
  }
}

function loadJson(pathname) {
  return JSON.parse(fs.readFileSync(pathname, 'utf8'));
}

function aggregateFromLogs(logsDir) {
  const files = fs.readdirSync(logsDir);
  const calibrations = new Map();
  const rows = [];

  for (const file of files) {
    if (!file.endsWith('.stdout.json')) continue;
    const fullPath = path.join(logsDir, file);
    const stderrPath = path.join(logsDir, file.replace(/\.stdout\.json$/, '.stderr.log'));
    const rowArray = loadJson(fullPath);
    if (!Array.isArray(rowArray) || rowArray.length !== 1) {
      throw new Error(`expected exactly one row in ${fullPath}`);
    }
    const row = rowArray[0];
    const stem = file.replace(/\.stdout\.json$/, '');
    const peakRssKib = fs.existsSync(stderrPath)
      ? parsePeakRssKib(fs.readFileSync(stderrPath, 'utf8'))
      : null;
    let match = stem.match(/^cal-srv(swift|rust)-(local|shm)-p(\d+)-i(\d+)$/);
    if (match) {
      const [, serverImpl, transport, payloadSizeRaw, inFlightRaw] = match;
      const payloadSize = Number(payloadSizeRaw);
      const inFlight = Number(inFlightRaw);
      const key = `${serverImpl}|${payloadSize}|${inFlight}`;
      let calibration = calibrations.get(key);
      if (!calibration) {
        calibration = {
          server_impl: serverImpl,
          payload_size: payloadSize,
          in_flight: inFlight,
          baseline_rps: null,
          transport_trials: {},
          offered_rps_values: [],
          load_factors: [],
        };
        calibrations.set(key, calibration);
      }
      calibration.transport_trials[transport] = {
        ...row,
        peak_rss_kib: peakRssKib,
        label: stem,
      };
      continue;
    }

    match = stem.match(/^cal-(local|shm)-p(\d+)-i(\d+)$/);
    if (match) {
      const [, transport, payloadSizeRaw, inFlightRaw] = match;
      const payloadSize = Number(payloadSizeRaw);
      const inFlight = Number(inFlightRaw);
      const key = `swift|${payloadSize}|${inFlight}`;
      let calibration = calibrations.get(key);
      if (!calibration) {
        calibration = {
          server_impl: 'swift',
          payload_size: payloadSize,
          in_flight: inFlight,
          baseline_rps: null,
          transport_trials: {},
          offered_rps_values: [],
          load_factors: [],
        };
        calibrations.set(key, calibration);
      }
      calibration.transport_trials[transport] = {
        ...row,
        peak_rss_kib: peakRssKib,
        label: stem,
      };
      continue;
    }

    match = stem.match(/^b(\d+)-o(\d+)-srv(swift|rust)-(local|shm)-p(\d+)-i(\d+)-r(\d+)$/);
    if (match) {
      const [, blockRaw, orderRaw, serverImpl, transport, payloadSizeRaw, inFlightRaw, offeredRpsRaw] = match;
      rows.push({
        ...row,
        block: Number(blockRaw),
        order_in_block: Number(orderRaw),
        server_impl: serverImpl,
        transport,
        payload_size: Number(payloadSizeRaw),
        in_flight: Number(inFlightRaw),
        offered_rps: Number(offeredRpsRaw),
        peak_rss_kib: peakRssKib,
        label: stem,
      });
      continue;
    }

    match = stem.match(/^b(\d+)-o(\d+)-(local|shm)-p(\d+)-i(\d+)-r(\d+)$/);
    if (!match) continue;
    const [, blockRaw, orderRaw, transport, payloadSizeRaw, inFlightRaw, offeredRpsRaw] = match;
    rows.push({
      ...row,
      block: Number(blockRaw),
      order_in_block: Number(orderRaw),
      server_impl: 'swift',
      transport,
      payload_size: Number(payloadSizeRaw),
      in_flight: Number(inFlightRaw),
      offered_rps: Number(offeredRpsRaw),
      peak_rss_kib: peakRssKib,
      label: stem,
    });
  }

  const sortedCalibrations = [...calibrations.values()].sort(
    (a, b) => a.server_impl.localeCompare(b.server_impl) || a.payload_size - b.payload_size || a.in_flight - b.in_flight,
  );
  for (const calibration of sortedCalibrations) {
    const transportTrials = Object.values(calibration.transport_trials);
    if (transportTrials.length > 0) {
      calibration.baseline_rps = Math.min(
        ...transportTrials.map((trial) => trial.offered_rps ?? trial.calls_per_sec),
      );
    }
    const matchingRows = rows
      .filter(
        (row) =>
          row.server_impl === calibration.server_impl &&
          row.payload_size === calibration.payload_size && row.in_flight === calibration.in_flight,
      )
      .sort((a, b) => a.offered_rps - b.offered_rps);
    const offeredRpsValues = [...new Set(matchingRows.map((row) => row.offered_rps))];
    calibration.offered_rps_values = offeredRpsValues;
    calibration.load_factors = offeredRpsValues.map((offeredRps) =>
      calibration.baseline_rps && calibration.baseline_rps > 0
        ? offeredRps / calibration.baseline_rps
        : null,
    );
  }

  for (const row of rows) {
    const calibration = calibrations.get(`${row.server_impl}|${row.payload_size}|${row.in_flight}`);
    row.baseline_rps = calibration?.baseline_rps ?? null;
    row.load_factor =
      row.baseline_rps && row.baseline_rps > 0 ? row.offered_rps / row.baseline_rps : null;
  }

  rows.sort(
    (a, b) =>
      a.block - b.block ||
      a.order_in_block - b.order_in_block ||
      a.server_impl.localeCompare(b.server_impl) ||
      a.payload_size - b.payload_size ||
      a.in_flight - b.in_flight ||
      a.offered_rps - b.offered_rps ||
      a.transport.localeCompare(b.transport),
  );

  return {
    calibrations: sortedCalibrations,
    rows,
  };
}

function runTrial(label, serverImpl, transport, clientArgs, logsDir) {
  const addr = addrForTransport(transport);
  const args = [
    '--subject-cmd', subjectCmdForServerImpl(serverImpl),
    '--subject-mode', subjectModeFor(serverImpl, transport),
    '--addr', addr,
    '--',
    '--addr', addr,
    ...clientArgs,
    '--json',
  ];
  const result = spawnSync('./target/release/examples/bench_runner', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });

  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  fs.writeFileSync(path.join(logsDir, `${label}.stdout.json`), stdout);
  fs.writeFileSync(path.join(logsDir, `${label}.stderr.log`), stderr);

  if (result.status !== 0) {
    throw new Error(`trial ${label} failed with status ${result.status}\n${stderr}`);
  }

  const rows = JSON.parse(stdout);
  if (!Array.isArray(rows) || rows.length !== 1) {
    throw new Error(`trial ${label} produced ${Array.isArray(rows) ? rows.length : 'non-array'} rows, expected 1`);
  }

  return {
    ...rows[0],
    server_impl: serverImpl,
    peak_rss_kib: parsePeakRssKib(stderr),
    label,
  };
}

function dropRate(row) {
  const issued = Number.isFinite(row.issued) ? row.issued : 0;
  const dropped = Number.isFinite(row.dropped) ? row.dropped : 0;
  const totalAttempts = issued + dropped;
  if (totalAttempts <= 0) return 1;
  return dropped / totalAttempts;
}

function isHealthyOpenLoop(row, dropThreshold) {
  return (row.errors ?? 0) === 0 && dropRate(row) <= dropThreshold;
}

function distanceToDropBand(row, minDrop, maxDrop) {
  if ((row.errors ?? 0) !== 0) return Number.POSITIVE_INFINITY;
  const drop = dropRate(row);
  if (drop < minDrop) return minDrop - drop;
  if (drop > maxDrop) return drop - maxDrop;
  return 0;
}

function chooseBestCalibrationProbe(probes, minDrop, maxDrop) {
  if (!probes.length) return null;
  const center = (minDrop + maxDrop) / 2;
  const sorted = probes.slice().sort((a, b) => {
    const da = distanceToDropBand(a.row, minDrop, maxDrop);
    const db = distanceToDropBand(b.row, minDrop, maxDrop);
    if (da !== db) return da - db;
    const ea = a.row.errors ?? 0;
    const eb = b.row.errors ?? 0;
    if (ea !== eb) return ea - eb;
    const ca = Math.abs(dropRate(a.row) - center);
    const cb = Math.abs(dropRate(b.row) - center);
    return ca - cb;
  });
  return sorted[0];
}

function copyTrialLogs(logsDir, fromLabel, toLabel) {
  if (fromLabel === toLabel) return;
  for (const ext of ['stdout.json', 'stderr.log']) {
    const from = path.join(logsDir, `${fromLabel}.${ext}`);
    const to = path.join(logsDir, `${toLabel}.${ext}`);
    if (fs.existsSync(from)) {
      fs.copyFileSync(from, to);
    }
  }
}

function runCalibrationProbe(args, serverImpl, transport, payloadSize, inFlight, offeredRps, logsDir, suffix) {
  return runTrial(
    `calprobe-srv${serverImpl}-${transport}-p${payloadSize}-i${inFlight}-r${offeredRps}-${suffix}`,
    serverImpl,
    transport,
    [
      '--workload', args.workload,
      '--payload-sizes', String(payloadSize),
      '--in-flights', String(inFlight),
      '--drive-mode', 'open',
      '--offered-rps', String(offeredRps),
      '--warmup-secs', String(args.calibrationWarmupSecs),
      '--measure-secs', String(args.calibrationMeasureSecs),
    ],
    logsDir,
  );
}

function calibrateTransportOpenLoop(args, serverImpl, transport, payloadSize, inFlight, logsDir) {
  const minDrop = args.calibrationTargetDropMin;
  const maxDrop = args.calibrationTargetDropMax;
  if (!(Number.isFinite(minDrop) && Number.isFinite(maxDrop) && minDrop >= 0 && maxDrop >= minDrop)) {
    throw new Error(`invalid drop band: min=${minDrop} max=${maxDrop}`);
  }

  let offered = Math.max(1, args.calibrationStartRps);
  const maxOffered = Math.max(offered, args.calibrationMaxRps);
  const maxProbes = Math.max(1, args.calibrationMaxProbes);
  const refineSteps = Math.max(0, args.calibrationRefineSteps);

  const probes = [];
  let step = 0;
  let low = null;
  let high = null;

  while (step < maxProbes) {
    const row = runCalibrationProbe(
      args,
      serverImpl,
      transport,
      payloadSize,
      inFlight,
      offered,
      logsDir,
      `scan${step}`,
    );
    const probe = { offered_rps: offered, row };
    probes.push(probe);

    const d = dropRate(row);
    if ((row.errors ?? 0) === 0 && d >= minDrop && d <= maxDrop) {
      return { bestOfferedRps: offered, row };
    }

    if ((row.errors ?? 0) !== 0 || d > maxDrop) {
      if (!high || offered < high.offered_rps) high = probe;
      if (low) break;
      const next = Math.max(1, Math.floor(offered / 2));
      if (next === offered) break;
      offered = next;
      step += 1;
      continue;
    }

    if (!low || offered > low.offered_rps) low = probe;
    const next = Math.min(maxOffered, Math.max(offered + 1, offered * 2));
    if (next === offered) break;
    offered = next;
    step += 1;
  }

  for (let i = 0; i < refineSteps; i++) {
    if (!low || !high) break;
    if (high.offered_rps - low.offered_rps <= 1) break;
    const mid = Math.floor((low.offered_rps + high.offered_rps) / 2);
    const row = runCalibrationProbe(args, serverImpl, transport, payloadSize, inFlight, mid, logsDir, `refine${i}`);
    const probe = { offered_rps: mid, row };
    probes.push(probe);
    const d = dropRate(row);
    if ((row.errors ?? 0) === 0 && d >= minDrop && d <= maxDrop) {
      return { bestOfferedRps: mid, row };
    }
    if ((row.errors ?? 0) !== 0 || d > maxDrop) {
      high = probe;
    } else {
      low = probe;
    }
  }

  const best = chooseBestCalibrationProbe(probes, minDrop, maxDrop);
  if (!best) {
    throw new Error(`no calibration probes produced a candidate for server=${serverImpl} transport=${transport} payload=${payloadSize} in_flight=${inFlight}`);
  }
  return {
    bestOfferedRps: Math.max(1, Math.round(best.offered_rps)),
    row: best.row,
  };
}

function calibrateCondition(args, serverImpl, payloadSize, inFlight, logsDir) {
  const perTransport = {};
  for (const transport of args.transports) {
    if (!supportsPair(serverImpl, transport)) {
      console.error(`skipping unsupported pair server=${serverImpl} transport=${transport}`);
      continue;
    }
    const calibrated = calibrateTransportOpenLoop(args, serverImpl, transport, payloadSize, inFlight, logsDir);
    const aliasLabel = `cal-srv${serverImpl}-${transport}-p${payloadSize}-i${inFlight}`;
    copyTrialLogs(logsDir, calibrated.row.label, aliasLabel);
    perTransport[transport] = { ...calibrated.row, label: aliasLabel };
  }

  const transportRows = Object.values(perTransport);
  if (transportRows.length === 0) {
    throw new Error(`no usable transports for server=${serverImpl} payload=${payloadSize} in_flight=${inFlight}`);
  }
  const baselineRps = Math.min(...transportRows.map((row) => row.offered_rps ?? row.calls_per_sec));
  const offeredRpsValues = [...new Set(args.loadFactors.map((factor) => Math.max(1, Math.round(baselineRps * factor))))].sort((a, b) => a - b);

  return {
    server_impl: serverImpl,
    payload_size: payloadSize,
    in_flight: inFlight,
    baseline_rps: baselineRps,
    transport_trials: perTransport,
    offered_rps_values: offeredRpsValues,
    load_factors: args.loadFactors,
  };
}

function main() {
  const args = parseArgs(process.argv);
  ensureDir(args.logsDir);
  if (args.aggregateOnly) {
    const aggregate = aggregateFromLogs(args.logsDir);
    fs.writeFileSync(args.out, JSON.stringify(aggregate, null, 2) + '\n');
    console.log(`wrote ${aggregate.rows.length} open-loop trial rows to ${args.out}`);
    return;
  }

  build();

  const calibrations = [];
  for (const serverImpl of args.serverImpls) {
    for (const payloadSize of args.payloadSizes) {
      for (const inFlight of args.inFlights) {
        console.error(`calibrating server=${serverImpl} payload=${payloadSize} in_flight=${inFlight}`);
        calibrations.push(calibrateCondition(args, serverImpl, payloadSize, inFlight, args.logsDir));
      }
    }
  }

  const rows = [];
  for (let block = 1; block <= args.blocks; block++) {
    const conditions = [];
    for (const cal of calibrations) {
      for (let i = 0; i < cal.offered_rps_values.length; i++) {
        for (const transport of args.transports) {
          if (!Object.prototype.hasOwnProperty.call(cal.transport_trials, transport)) {
            continue;
          }
          conditions.push({
            transport,
            server_impl: cal.server_impl,
            payload_size: cal.payload_size,
            in_flight: cal.in_flight,
            offered_rps: cal.offered_rps_values[i],
            load_factor: cal.load_factors[i],
            baseline_rps: cal.baseline_rps,
          });
        }
      }
    }

    const randomized = shuffle(conditions);
    randomized.forEach((condition, idx) => {
      console.error(`block ${block} trial ${idx + 1}/${randomized.length}: server=${condition.server_impl} transport=${condition.transport} payload=${condition.payload_size} in_flight=${condition.in_flight} offered_rps=${condition.offered_rps}`);
      const row = runTrial(
        `b${block}-o${idx + 1}-srv${condition.server_impl}-${condition.transport}-p${condition.payload_size}-i${condition.in_flight}-r${condition.offered_rps}`,
        condition.server_impl,
        condition.transport,
        [
          '--workload', args.workload,
          '--payload-sizes', String(condition.payload_size),
          '--in-flights', String(condition.in_flight),
          '--drive-mode', 'open',
          '--offered-rps', String(condition.offered_rps),
          '--warmup-secs', String(args.warmupSecs),
          '--measure-secs', String(args.measureSecs),
        ],
        args.logsDir,
      );
      rows.push({
        ...row,
        block,
        order_in_block: idx + 1,
        server_impl: condition.server_impl,
        baseline_rps: condition.baseline_rps,
        load_factor: condition.load_factor,
      });
    });
  }

  fs.writeFileSync(args.out, JSON.stringify({ calibrations, rows }, null, 2) + '\n');
  console.log(`wrote ${rows.length} open-loop trial rows to ${args.out}`);
}

main();
