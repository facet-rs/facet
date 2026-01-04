+++
title = "Errors"
description = "Non-normative implementation guidance for error handling"
weight = 90
+++

This document is **non-normative**. For normative semantics, see [Error Handling & Retries](@/spec/errors.md).

## Guidance

- Use the standard error codes for standard conditions.
- Set the `ERROR` flag consistently with `status.code != 0` (if you use it).
- Include `Status` in all error responses.
- Handle unknown error codes gracefully.
- Populate `details` for actionable errors; include `message` for debugging.
- Implement exponential backoff for retries.
- Use an application-specific error space (for example 400+) when you need custom codes; avoid including stack traces in production.
