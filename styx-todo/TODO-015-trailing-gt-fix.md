# TODO-015: Merge Trailing `>` Error Handling Fix

## Status
TODO

## Description
PR #2 (https://github.com/bearcove/styx/pull/2) fixes a bug where trailing `>` in invalid attribute syntax was silently ignored.

## The Fix
Previously, malformed syntax like `key value>` would be silently accepted. The PR makes both Rust and Go parsers emit an "expected a value" error when `>` appears without a valid attribute value.

## Action Required
- Rebase PR #2 onto main
- Resolve any conflicts
- Merge to main

## Notes
PR also includes Monaco editor improvements (vim mode initialization).
