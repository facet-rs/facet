import { execSync } from "child_process";

const BASE_URL = "https://crates.io";
const USER_AGENT = "facet-trusted-publishing-setup (contact: amos@bearcove.eu)";

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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

const TOKEN = process.env.CRATES_IO_TOKEN;
if (!TOKEN) {
  console.error("Set CRATES_IO_TOKEN environment variable");
  process.exit(1);
}

const crates = getPublishableCrates();
console.log(`Found ${crates.length} publishable crates\n`);

// Check which crates have never been published (concurrently)
console.log("Checking which crates exist on crates.io...");
const existsResults = await Promise.all(
  crates.map(async (name) => ({ name, exists: await crateExists(name) })),
);

const unpublished = existsResults.filter((r) => !r.exists).map((r) => r.name);
if (unpublished.length > 0) {
  console.error(`\nThe following crates have never been published to crates.io:`);
  for (const name of unpublished) {
    console.error(`  - ${name}`);
  }
  console.error(
    `\nYou must publish these crates manually first before setting up trusted publishing.`,
  );
  process.exit(1);
}

console.log("All crates exist on crates.io.\n");

for (const crate of crates) {
  console.log(`Configuring trusted publishing for ${crate}...`);
  try {
    await createTrustpubGithubConfig(
      {
        github_config: {
          crate,
          repository_owner: "facet-rs",
          repository_name: "facet",
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
