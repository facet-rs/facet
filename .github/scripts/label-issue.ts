/**
 * AI-powered issue labeler using GitHub Models API
 *
 * This script analyzes GitHub issues and applies appropriate labels
 * using the GitHub Models inference API (free tier, no API key needed).
 */

import { Octokit } from "@octokit/rest";

// Available labels in the facet repo
const AVAILABLE_LABELS = [
  "🐛 bug",
  "✨ enhancement",
  "✋ question",
  "📒 documentation",
  "📁 formats",
  "📜 derive",
  "🍪 nostd",
  "💨 performance",
  "🎺 soundness",
  "💅 devex",
  "🧹 code quality",
] as const;

type Label = (typeof AVAILABLE_LABELS)[number];

interface GitHubModelsResponse {
  choices: Array<{
    message: {
      content: string;
    };
  }>;
}

async function callGitHubModels(
  token: string,
  issueTitle: string,
  issueBody: string
): Promise<Label[]> {
  const response = await fetch(
    "https://models.github.ai/inference/chat/completions",
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        model: "openai/gpt-4o-mini",
        messages: [
          {
            role: "system",
            content: `You are a GitHub issue classifier for the "facet" Rust library.
facet is a reflection and serialization framework for Rust.

Analyze the issue and return a JSON array of applicable labels. Choose labels that accurately describe the issue.

Available labels and their meanings:
- "🐛 bug": Something is broken, crashes, errors, panics, regressions
- "✨ enhancement": Feature requests, improvements, new functionality
- "✋ question": Questions about usage, how things work, requests for help
- "📒 documentation": Docs improvements, typos, README updates, examples
- "📁 formats": Related to serialization formats (JSON, YAML, TOML, XML, KDL, msgpack, postcard, CSV, etc.)
- "📜 derive": Related to the derive macro, proc-macros, #[facet(...)] attributes
- "🍪 nostd": no_std support, embedded, alloc-only environments
- "💨 performance": Speed, benchmarks, optimization
- "🎺 soundness": Unsafe code, undefined behavior, miri, memory safety
- "💅 devex": Developer experience, ergonomics, API design
- "🧹 code quality": Refactoring, cleanup, technical debt

Rules:
1. Return ONLY a valid JSON array of strings, nothing else
2. Only use labels from the list above (exact match including emoji)
3. Apply 1-3 labels that best fit the issue
4. If unsure, prefer fewer labels over more

Example response: ["🐛 bug", "📁 formats"]`,
          },
          {
            role: "user",
            content: `Issue Title: ${issueTitle}\n\nIssue Body:\n${issueBody || "(no body)"}`,
          },
        ],
        max_tokens: 150,
        temperature: 0.1, // Low temperature for consistent classification
      }),
    }
  );

  if (!response.ok) {
    const error = await response.text();
    throw new Error(`GitHub Models API error: ${response.status} - ${error}`);
  }

  const data = (await response.json()) as GitHubModelsResponse;
  const content = data.choices[0]?.message?.content?.trim();

  if (!content) {
    throw new Error("Empty response from GitHub Models");
  }

  // Parse the JSON response
  let labels: string[];
  try {
    labels = JSON.parse(content);
  } catch {
    // Sometimes the model wraps it in markdown code blocks
    const match = content.match(/\[[\s\S]*\]/);
    if (match) {
      labels = JSON.parse(match[0]);
    } else {
      throw new Error(`Failed to parse labels from response: ${content}`);
    }
  }

  // Validate labels against our allowed list
  const validLabels = labels.filter((label): label is Label =>
    AVAILABLE_LABELS.includes(label as Label)
  );

  return validLabels;
}

const FALLBACK_LABEL = "needs-triage";

async function main() {
  const token = process.env.GITHUB_TOKEN;
  const repo = process.env.GITHUB_REPOSITORY;
  const issueNumber = process.env.ISSUE_NUMBER;
  const issueTitle = process.env.ISSUE_TITLE;
  const issueBody = process.env.ISSUE_BODY;

  if (!token) throw new Error("GITHUB_TOKEN is required");
  if (!repo) throw new Error("GITHUB_REPOSITORY is required");
  if (!issueNumber) throw new Error("ISSUE_NUMBER is required");
  if (!issueTitle) throw new Error("ISSUE_TITLE is required");

  const [owner, repoName] = repo.split("/");
  const octokit = new Octokit({ auth: token });

  console.log(`Analyzing issue #${issueNumber}: ${issueTitle}`);

  let labels: Label[];

  try {
    labels = await callGitHubModels(token, issueTitle, issueBody || "");
  } catch (error) {
    // AI failed — fall back to needs-triage so the issue doesn't slip through
    console.error(`AI labeling failed: ${error instanceof Error ? error.message : error}`);
    console.log(`Falling back to "${FALLBACK_LABEL}" label`);

    await octokit.rest.issues.addLabels({
      owner,
      repo: repoName,
      issue_number: parseInt(issueNumber, 10),
      labels: [FALLBACK_LABEL],
    });

    // Add a comment so maintainers know AI labeling failed
    await octokit.rest.issues.createComment({
      owner,
      repo: repoName,
      issue_number: parseInt(issueNumber, 10),
      body: `⚠️ Automatic labeling failed (likely rate limit). This issue needs manual triage.`,
    });

    return;
  }

  if (labels.length === 0) {
    console.log(`No labels determined, applying "${FALLBACK_LABEL}"`);
    await octokit.rest.issues.addLabels({
      owner,
      repo: repoName,
      issue_number: parseInt(issueNumber, 10),
      labels: [FALLBACK_LABEL],
    });
    return;
  }

  console.log(`Applying labels: ${labels.join(", ")}`);

  await octokit.rest.issues.addLabels({
    owner,
    repo: repoName,
    issue_number: parseInt(issueNumber, 10),
    labels,
  });

  console.log("Labels applied successfully");
}

main().catch((error) => {
  console.error("Fatal error:", error.message);
  process.exit(1);
});
