#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function readVoxPackageVersion(cargoTomlSource) {
  const match = cargoTomlSource.match(/\[package\][\s\S]*?\nversion = "([^"]+)"/);
  if (!match?.[1]) {
    throw new Error("Could not determine vox package version from Cargo.toml");
  }
  return match[1];
}

export function discoverPublicTypeScriptPackages(repoRoot) {
  const packagesDir = join(repoRoot, "typescript", "packages");
  return readdirSync(packagesDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(packagesDir, entry.name, "package.json"))
    .map((manifestPath) => ({
      manifestPath,
      manifest: JSON.parse(readFileSync(manifestPath, "utf8")),
    }))
    .filter(({ manifest }) => manifest.private !== true);
}

export function syncTypeScriptPackageVersions(repoRoot) {
  const cargoTomlPath = join(repoRoot, "rust", "vox", "Cargo.toml");
  const version = readVoxPackageVersion(readFileSync(cargoTomlPath, "utf8"));
  const packages = discoverPublicTypeScriptPackages(repoRoot);

  const updatedPackages = [];
  for (const { manifestPath, manifest } of packages) {
    if (manifest.version === version) {
      continue;
    }

    const previousVersion = manifest.version;
    manifest.version = version;
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    updatedPackages.push({ name: manifest.name, previousVersion, version });
  }

  return { version, updatedPackages };
}

const entrypoint = process.argv[1] ? resolve(process.argv[1]) : null;
if (entrypoint === fileURLToPath(import.meta.url)) {
  const repoRoot = process.cwd();
  const { version, updatedPackages } = syncTypeScriptPackageVersions(repoRoot);
  if (updatedPackages.length === 0) {
    console.log(`TypeScript package versions already match vox crate version ${version}`);
  } else {
    console.log(`Synced ${updatedPackages.length} TypeScript packages to ${version}`);
    for (const pkg of updatedPackages) {
      console.log(`- ${pkg.name}: ${pkg.previousVersion} -> ${pkg.version}`);
    }
  }
}
