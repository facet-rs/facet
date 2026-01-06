import { execSync } from "child_process";
import { mkdtempSync, writeFileSync, readFileSync, existsSync } from "fs";
import { tmpdir, homedir } from "os";
import { join } from "path";
import * as readline from "readline";
import { exit } from "process";

const BASE_URL = "https://crates.io";
const USER_AGENT = "facet-trusted-publishing-setup (contact: amos@bearcove.eu)";

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function askConfirmation(question: string): Promise<boolean> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise((resolve) => {
    rl.question(`${question} (y/n): `, (answer) => {
      rl.close();
      resolve(answer.toLowerCase() === "y" || answer.toLowerCase() === "yes");
    });
  });
}

function getCargoToken(): string | null {
  const credentialsPath = join(homedir(), ".cargo", "credentials.toml");
  if (!existsSync(credentialsPath)) {
    return null;
  }

  try {
    const content = readFileSync(credentialsPath, "utf-8");
    const match = content.match(/token\s*=\s*"([^"]+)"/);
    return match ? match[1] : null;
  } catch (error) {
    console.error(`Error reading credentials: ${error}`);
    return null;
  }
}

function getGitHubRepo(): { owner: string; name: string } | null {
  try {
    const remote = execSync("git remote get-url origin", {
      encoding: "utf-8",
    }).trim();

    // Match GitHub URLs in various formats:
    // - https://github.com/owner/repo.git
    // - git@github.com:owner/repo.git
    // - https://github.com/owner/repo
    const httpsMatch = remote.match(/github\.com[/:]([^/]+)\/([^/.]+)(\.git)?$/);
    if (httpsMatch) {
      return { owner: httpsMatch[1], name: httpsMatch[2] };
    }

    return null;
  } catch (error) {
    console.error(`Error reading git remote: ${error}`);
    return null;
  }
}

function publishStubCrate(name: string, token: string) {
  // Create temporary directory
  const tmpDir = mkdtempSync(join(tmpdir(), `${name}-stub-`));
  console.log(`  Creating stub in ${tmpDir}`);

  // Create minimal Cargo.toml
  const cargoToml = `[package]
name = "${name}"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/bearcove/roam"
description = "Stub package for ${name}"

[lib]
path = "lib.rs"
`;

  writeFileSync(join(tmpDir, "Cargo.toml"), cargoToml);

  // Create minimal lib.rs
  const libRs = `// Stub package for ${name}
// This will be replaced with the actual implementation in a future release.
`;

  writeFileSync(join(tmpDir, "lib.rs"), libRs);

  // Publish with token (using environment variable)
  console.log(`  Publishing ${name} v0.1.0...`);
  execSync(`cargo publish --allow-dirty`, {
    cwd: tmpDir,
    stdio: "inherit",
    env: { ...process.env, CARGO_REGISTRY_TOKEN: token },
  });

  console.log(`  ✓ Published ${name} v0.1.0`);
}

async function crateExists(name: string): Promise<boolean> {
  const res = await fetch(`${BASE_URL}/api/v1/crates/${name}`, {
    headers: { "User-Agent": USER_AGENT },
  });
  return res.status === 200;
}

async function createTrustpubGithubConfig(
  body: {
    github_config: {
      crate: string;
      repository_owner: string;
      repository_name: string;
      workflow_filename: string;
    };
  },
  options: { headers: Record<string, string> },
) {
  const res = await fetch(`${BASE_URL}/api/v1/trusted_publishing/github_configs`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "User-Agent": USER_AGENT, ...options.headers },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${res.status}: ${text}`);
  }
  return res.json();
}

interface CargoMetadata {
  packages: Array<{
    name: string;
    id: string;
    publish: string[] | null;
  }>;
  workspace_members: string[];
}

function getPublishableCrates(): string[] {
  const metadata: CargoMetadata = JSON.parse(
    execSync("cargo metadata --format-version=1 --no-deps", {
      encoding: "utf-8",
    }),
  );

  const workspaceMemberIds = new Set(metadata.workspace_members);

  return metadata.packages
    .filter((pkg) => {
      if (!workspaceMemberIds.has(pkg.id)) return false;
      if (pkg.publish !== null && pkg.publish.length === 0) return false;
      return true;
    })
    .map((pkg) => pkg.name);
}

const crates = getPublishableCrates();
console.log(`Found ${crates.length} publishable crates\n`);

// Check which crates have never been published (concurrently)
console.log("Checking which crates exist on crates.io...");
const existsResults = await Promise.all(
  crates.map(async (name) => ({ name, exists: await crateExists(name) })),
);

const unpublished = existsResults.filter((r) => !r.exists).map((r) => r.name);

let TOKEN = process.env.CRATES_IO_TOKEN;
if (!TOKEN) {
  TOKEN = getCargoToken();
  if (!TOKEN) {
    console.error("No cargo token found. Either:");
    console.error("  - Set CRATES_IO_TOKEN environment variable");
    console.error("  - Run `cargo login` to save token in ~/.cargo/credentials.toml");
    process.exit(1);
  }
  console.log("Using token from ~/.cargo/credentials.toml\n");
}

// Get GitHub repository info
const repo = getGitHubRepo();
if (!repo) {
  console.error("Could not determine GitHub repository from git remote");
  process.exit(1);
}
console.log(`GitHub repository: ${repo.owner}/${repo.name}\n`);

// Check that release-plz.yml exists
const workflowPath = ".github/workflows/release-plz.yml";
if (!existsSync(workflowPath)) {
  console.error(`Error: ${workflowPath} does not exist`);
  console.error("Trusted publishing requires this workflow file to exist");
  process.exit(1);
}

if (unpublished.length > 0) {
  console.log(`\nThe following crates have never been published to crates.io:`);
  for (const name of unpublished) {
    console.log(`  - ${name}`);
  }

  const shouldPublish = await askConfirmation(
    `\nDo you want to publish stub versions (0.1.0) of these ${unpublished.length} crates?`,
  );

  if (!shouldPublish) {
    console.log("\nSkipping stub publication. You must publish these crates manually first.");
    process.exit(1);
  }

  console.log("\nPublishing stub versions...\n");
  for (const name of unpublished) {
    try {
      publishStubCrate(name, TOKEN);
    } catch (error) {
      console.error(`  ✗ Failed to publish ${name}: ${error}`);
      process.exit(1);
    }
    // Rate limit: wait between publishes
    await sleep(1100);
  }

  console.log("\nAll stub crates published successfully!\n");
}

console.log("All crates exist on crates.io.\n");

for (const crate of crates) {
  console.log(`Configuring trusted publishing for ${crate}...`);
  try {
    await createTrustpubGithubConfig(
      {
        github_config: {
          crate,
          repository_owner: repo.owner,
          repository_name: repo.name,
          workflow_filename: "release-plz.yml",
        },
      },
      {
        headers: { Authorization: TOKEN },
      },
    );
    console.log(`  ✓ ${crate}`);
  } catch (error) {
    console.error(`  ✗ ${crate}: ${error}`);
  }
  await sleep(1100);
}

console.log("\nDone! Trusted publishing configured for all crates.");
