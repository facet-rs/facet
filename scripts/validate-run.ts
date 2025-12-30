#!/usr/bin/env -S pnpx tsx
// Validate run.json against the TypeScript types
// Usage: pnpx tsx validate-run.ts path/to/run.json

import { readFileSync } from 'fs';

// Types from run-types.d.ts (will be generated on-the-fly)
interface RunJson {
  schema?: string;
  run: RunMeta;
  defaults?: RunDefaults;
  catalog?: RunCatalog;
  results: RunResults;
}

interface RunMeta {
  run_id: string;
  branch_key: string;
  branch_original?: string;
  sha?: string;
  commit?: string;
  short?: string;
  commit_short?: string;
  timestamp?: string;
  generated_at?: string;
  timestamp_unix?: number;
  commit_message: string;
  pr_number?: string;
  pr_title?: string;
}

interface RunDefaults {
  operation: string;
  metric: string;
  baseline_target: string;
  primary_target: string;
  comparison_mode: string;
}

interface RunCatalog {
  formats_order: string[];
  formats: Record<string, FormatDef>;
  groups_order: string[];
  groups: Record<string, GroupDef>;
  benchmarks: Record<string, BenchmarkDef>;
  targets: Record<string, TargetDef>;
  metrics: Record<string, MetricDef>;
}

interface FormatDef {
  key: string;
  label: string;
  baseline_target: string;
  primary_target: string;
}

interface GroupDef {
  label: string;
  benchmarks_order: string[];
}

interface BenchmarkDef {
  key: string;
  label: string;
  group: string;
  format: string;
  targets_order: string[];
  metrics_order: string[];
}

interface TargetDef {
  key: string;
  label: string;
  kind: string;
}

interface MetricDef {
  key: string;
  label: string;
  unit: string;
  better: string;
}

interface RunResults {
  values: Record<string, BenchmarkOps>;
  errors: RunErrors;
}

interface BenchmarkOps {
  deserialize: Record<string, TargetMetrics | null>;
  serialize: Record<string, TargetMetrics | null>;
}

interface TargetMetrics {
  instructions?: number;
  estimated_cycles?: number;
  time_median_ns?: number;
  l1_hits?: number;
  ll_hits?: number;
  ram_hits?: number;
  total_read_write?: number;
  tier2_attempts?: number;
  tier2_successes?: number;
  tier2_compile_unsupported?: number;
  tier2_runtime_unsupported?: number;
  tier2_runtime_error?: number;
  tier1_fallbacks?: number;
}

interface RunErrors {
  _parse_failures?: {
    divan: string[];
    gungraun: string[];
  };
}

function validateRunJson(data: unknown): data is RunJson {
  if (typeof data !== 'object' || data === null) {
    console.error('Error: run.json must be an object');
    return false;
  }

  const obj = data as Record<string, unknown>;

  // Check required fields
  if (!obj.run || typeof obj.run !== 'object') {
    console.error('Error: missing or invalid "run" field');
    return false;
  }

  if (!obj.results || typeof obj.results !== 'object') {
    console.error('Error: missing or invalid "results" field');
    return false;
  }

  const run = obj.run as Record<string, unknown>;
  if (typeof run.run_id !== 'string') {
    console.error('Error: run.run_id must be a string');
    return false;
  }
  if (typeof run.branch_key !== 'string') {
    console.error('Error: run.branch_key must be a string');
    return false;
  }
  if (typeof run.commit_message !== 'string') {
    console.error('Error: run.commit_message must be a string');
    return false;
  }

  const results = obj.results as Record<string, unknown>;
  if (!results.values || typeof results.values !== 'object') {
    console.error('Error: results.values must be an object');
    return false;
  }

  // Validate catalog if present
  if (obj.catalog) {
    const catalog = obj.catalog as Record<string, unknown>;
    if (!Array.isArray(catalog.formats_order)) {
      console.error('Error: catalog.formats_order must be an array');
      return false;
    }
    if (!Array.isArray(catalog.groups_order)) {
      console.error('Error: catalog.groups_order must be an array');
      return false;
    }
  }

  return true;
}

// Main
const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('Usage: pnpx tsx validate-run.ts <path/to/run.json>');
  process.exit(1);
}

const runJsonPath = args[0];
try {
  const content = readFileSync(runJsonPath, 'utf-8');
  const data = JSON.parse(content);
  
  if (validateRunJson(data)) {
    console.log(`✓ ${runJsonPath} is valid`);
    console.log(`  Schema: ${data.schema || 'not specified'}`);
    console.log(`  Run ID: ${data.run.run_id}`);
    console.log(`  Benchmarks: ${Object.keys(data.results.values).length}`);
    if (data.catalog) {
      console.log(`  Formats: ${data.catalog.formats_order.join(', ')}`);
    }
    process.exit(0);
  } else {
    process.exit(1);
  }
} catch (e) {
  console.error(`Error reading ${runJsonPath}: ${e}`);
  process.exit(1);
}
