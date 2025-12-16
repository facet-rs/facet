#!/usr/bin/env node
/**
 * Copy performance benchmark reports to the perf repository
 *
 * This script does everything the old bash mess did:
 * - Copies benchmark reports to the right location
 * - Extracts git metadata (commit, branch, PR info)
 * - Generates metadata.json with proper JSON escaping
 * - Copies fonts, scripts, and favicons
 * - Updates the "latest" symlink
 */

const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function runCommand(cmd, args = []) {
  try {
    return execFileSync(cmd, args, { encoding: 'utf8' }).trim();
  } catch (e) {
    return '';
  }
}

function copyFile(src, dest) {
  try {
    fs.copyFileSync(src, dest);
    return true;
  } catch (e) {
    return false;
  }
}

function copyGlob(srcDir, pattern, destDir) {
  try {
    const files = fs.readdirSync(srcDir);
    let count = 0;
    for (const file of files) {
      if (file.match(pattern)) {
        copyFile(path.join(srcDir, file), path.join(destDir, file));
        count++;
      }
    }
    return count;
  } catch (e) {
    return 0;
  }
}

function main() {
  // Get configuration from environment
  const perfRoot = process.env.PERF_ROOT || '/tmp/perf';
  const branchOriginal = process.env.BRANCH_ORIGINAL || runCommand('git', ['branch', '--show-current']);
  const branch = branchOriginal.replace(/[^a-zA-Z0-9-]/g, '_');

  // Use environment variables if provided (from CI), otherwise extract from git
  const commit = process.env.COMMIT || runCommand('git', ['rev-parse', 'HEAD']);
  const commitShort = process.env.COMMIT_SHORT || runCommand('git', ['rev-parse', '--short', 'HEAD']);
  const prNumber = process.env.PR_NUMBER || '';

  console.log(`📦 Copying reports for ${branch}@${commitShort}`);

  // Create destination directory
  const dest = path.join(perfRoot, branch, commit);
  fs.mkdirSync(dest, { recursive: true });
  console.log(`   Created ${dest}`);

  // Copy benchmark reports
  const benchReports = 'bench-reports';
  copyGlob(benchReports, /^report-.*\.html$/, dest);
  console.log(`   ✓ Copied HTML reports`);

  copyGlob(benchReports, /\.txt$/, dest);
  copyGlob(benchReports, /^perf-data-.*\.json$/, dest);
  console.log(`   ✓ Copied data files`);

  // Extract metadata
  const now = new Date();
  const timestamp = now.toISOString();
  const timestampDisplay = now.toISOString().replace('T', ' ').replace(/\.\d+Z$/, ' UTC');

  // Get commit message - use env var if provided (from CI), otherwise extract from git
  // CI passes the correct commit message for the actual PR HEAD, not the merge commit
  const commitMessage = process.env.COMMIT_MESSAGE || runCommand('git', ['log', '-1', '--format=%B', commit]);

  // Get PR title if this is a PR
  let prTitle = '';
  if (prNumber) {
    prTitle = runCommand('gh', ['pr', 'view', prNumber, '--json', 'title', '--jq', '.title']);
  }

  // Write metadata.json
  const metadata = {
    commit,
    commit_short: commitShort,
    branch,
    branch_original: branchOriginal,
    pr_number: prNumber,
    timestamp,
    timestamp_display: timestampDisplay,
    commit_message: commitMessage,
    pr_title: prTitle,
  };

  fs.writeFileSync(
    path.join(dest, 'metadata.json'),
    JSON.stringify(metadata, null, 2) + '\n'
  );
  console.log(`   ✓ Generated metadata.json`);

  // Copy fonts to shared location
  const fontsDir = path.join(perfRoot, 'fonts');
  fs.mkdirSync(fontsDir, { recursive: true });

  try {
    const files = fs.readdirSync(benchReports);
    for (const file of files) {
      if (file.endsWith('.ttf')) {
        copyFile(path.join(benchReports, file), path.join(fontsDir, file));
      }
    }
    console.log(`   ✓ Copied fonts`);
  } catch (e) {
    // Fonts are optional
  }

  // Copy scripts to root
  copyFile('scripts/perf-nav.js', path.join(perfRoot, 'nav.js'));
  copyFile('scripts/app.js', path.join(perfRoot, 'app.js'));
  console.log(`   ✓ Copied scripts`);

  // Copy favicon files
  copyFile('docs/static/favicon.png', path.join(perfRoot, 'favicon.png'));
  copyFile('docs/static/favicon.ico', path.join(perfRoot, 'favicon.ico'));
  console.log(`   ✓ Copied favicons`);

  // Update "latest" symlink
  const branchDir = path.join(perfRoot, branch);
  const latestLink = path.join(branchDir, 'latest');

  try {
    fs.unlinkSync(latestLink);
  } catch (e) {
    // Ignore if doesn't exist
  }

  fs.symlinkSync(commit, latestLink);
  console.log(`   ✓ Updated latest symlink`);

  console.log(`✅ Done!`);
  if (prNumber && prTitle) {
    console.log(`   PR #${prNumber}: ${prTitle}`);
  }
  console.log(`   Commit: ${commitMessage.split('\n')[0]}`);
}

main();
