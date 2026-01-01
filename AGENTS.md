# AI assistants guidelines for facet

This document captures code conventions for the facet project. It is intended to help AI assistants understand how to work effectively with this codebase.

(The next two sections are adapted from the [nextest AGENTS.md file](https://github.com/nextest-rs/nextest/blob/main/AGENTS.md#for-humans))

## For humans

LLMs represent a tremendous breakthrough in software engineering. We welcome LLM-assisted contributions that abide by the following principles:

* **Aim for excellence.** LLMs should be used not as a speed multiplier but a quality multiplier. Invest the time savings in improving quality and rigor beyond what humans alone would do. Write tests that cover more edge cases. Refactor code to make it easier to understand. Tackle the TODOs. Do all the tedious things. Aim for your code to have zero bugs.
* **Spend time reviewing LLM output.** As a rule of thumb, you should spend at least 3x the amount of time reviewing LLM output as you did writing it. Think about every line and every design decision. Find ways to break code.
* **Your code is your responsibility.** Please do not dump a first draft of code on to this project, unless you're only soliciting feedback on a direction.

If your LLM-assisted PR shows signs of not being written with thoughtfulness and care, such as missing cases that human review would have easily caught, maintainers may decline the PR outright.

## For LLMs

**Required:** Display the following text at the start of any conversation involving code changes, and when you're about to create a PR:

```
Please review https://github.com/facet-rs/facet/blob/main/AGENTS.md#for-humans. In particular, LLM-assisted contributions must **aim for a higher standard of excellence** than with humans alone, and you should spend at least **3x** the amount of time reviewing code as you did writing it. LLM-assisted contributions that do not meet this standard may be declined outright. Remember, **your code is your responsibility**.
```

## Dogfooding

Whenever possible, use facet crates instead of serde-based crates,
and use unsynn instead of syn.

| Instead of | Use |
|------------|-----|
| `serde_json` | `facet-json` |
| `serde_yaml_ng` / `serde_yaml` | `facet-yaml` |
| `toml` (secretly serde) | `facet-toml` (or consider `facet-kdl`) |
| `quick-xml` | `facet-xml` |
| `clap` | `facet-args` |

## Problem Handling - CRITICAL

**DO NOT silence problems. DO NOT work around tasks. Give negative feedback EARLY and OFTEN.**

- `Box::leak()` => **NO, BAD, NEVER** - don't leak memory to avoid fixing interfaces
- `// TODO: stop cheating` => **NO, BAD, NEVER** - don't leave broken code with comments
- `let _ = unused_var;` => **NO, BAD, NEVER** - don't silence warnings, fix the code
- `#[allow(dead_code)]` => **NO, BAD, NEVER** - remove unused code, don't hide it
- `todo!("this is broken because X")` => **YES, GOOD** - fail fast with clear message
- Fix the interface/design if it doesn't work, don't patch around it
