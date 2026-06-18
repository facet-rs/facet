#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FILE = path.join(__dirname, "INVESTIGATION.md");
const HEADER_RE = /^## (.+?) — (\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} \w+)$/;

function parseEntries(content) {
  const lines = content.split("\n");
  const entries = [];
  let current = null;

  for (const line of lines) {
    const m = line.match(HEADER_RE);
    if (m) {
      if (current) entries.push(current);
      current = { name: m[1], date: m[2], lines: [] };
    } else if (current) {
      current.lines.push(line);
    }
  }
  if (current) entries.push(current);

  for (const e of entries) {
    while (e.lines.length && e.lines[0].trim() === "") e.lines.shift();
    while (e.lines.length && e.lines[e.lines.length - 1].trim() === "") e.lines.pop();
    e.body = e.lines.join("\n");
    delete e.lines;
  }
  return entries;
}

function formatEntry(e) {
  return `## ${e.name} — ${e.date}\n\n${e.body}`;
}

function now() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  const offset = 1;
  const utc = d.getTime() + d.getTimezoneOffset() * 60000;
  const cet = new Date(utc + offset * 3600000);
  return `${cet.getFullYear()}-${pad(cet.getMonth() + 1)}-${pad(cet.getDate())} ${pad(cet.getHours())}:${pad(cet.getMinutes())}:${pad(cet.getSeconds())} CET`;
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf-8").trim();
}

const name = process.argv[2];
if (!name) {
  console.error("Usage: report.js NAME < message");
  process.exit(1);
}

const body = await readStdin();
if (!body) {
  console.error("Nothing on stdin");
  process.exit(1);
}

let content = "";
try { content = fs.readFileSync(FILE, "utf-8"); } catch {}

const entries = parseEntries(content);

let lastIdx = -1;
for (let i = entries.length - 1; i >= 0; i--) {
  if (entries[i].name === name) { lastIdx = i; break; }
}

const since = entries.slice(lastIdx + 1);

entries.push({ name, date: now(), body });

const out = "# Investigation Log\n\n" + entries.map(formatEntry).join("\n\n") + "\n";
fs.writeFileSync(FILE, out);

execSync(`git add ${FILE} && git commit -m "investigation: ${name}"`, {
  cwd: __dirname,
  stdio: "pipe",
});

if (since.length === 0) {
  console.log("(no new entries since your last report)");
} else {
  console.log(`--- ${since.length} entry/entries since your last report ---\n`);
  for (const e of since) {
    console.log(formatEntry(e));
    console.log();
  }
}
