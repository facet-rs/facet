const BASE_URL = "https://crates.io";
const USER_AGENT = "facet-trusted-publishing-setup (contact: amos@bearcove.eu)";

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function createTrustpubGithubConfig(
  body: { github_config: { crate: string; repository_owner: string; repository_name: string; workflow_filename: string } },
  options: { headers: Record<string, string> }
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

const CRATES = [
  "facet-core",
  "facet",
  "facet-macros-impl",
  "facet-macros",
  "facet-macro-types",
  "facet-macro-parse",
  "facet-error",
  "facet-miette",
  "facet-default",
  "facet-reflect",
  "facet-solver",
  "facet-testhelpers",
  "facet-testhelpers-macros",
  "facet-pretty",
  "facet-args",
  "facet-urlencoded",
  "facet-axum",
  "cinereus",
  "facet-diff-core",
  "facet-diff",
  "facet-assert",
  "facet-value",
  "facet-singularize",
  "facet-path",
  "facet-shapelike",
  "facet-format",
  "facet-json",
  "facet-postcard",
  "facet-msgpack",
  "facet-xml",
  "facet-svg",
  "facet-toml",
  "facet-yaml",
  "facet-asn1",
  "facet-csv",
  "facet-kdl",
  "facet-xdr",
  "facet-html",
  "facet-html-dom",
  "facet-json-schema",
  "facet-typescript",
];

const TOKEN = process.env.CRATES_IO_TOKEN;
if (!TOKEN) {
  console.error("Set CRATES_IO_TOKEN environment variable");
  process.exit(1);
}

for (const crate of CRATES) {
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
      }
    );
    console.log(`  ✓ ${crate}`);
  } catch (error) {
    console.error(`  ✗ ${crate}: ${error}`);
  }
  await sleep(1100);
}

console.log("\nDone! Trusted publishing configured for all crates.");
