+++
title = "Cancellation & Deadlines"
description = "Non-normative implementation guidance for cancellation and deadlines"
weight = 80
+++

This document is **non-normative**. For normative semantics, see [Cancellation & Deadlines](@/spec/cancellation.md).

## Cancellation

- Support the `CancelChannel` control message.
- Propagate cancellation to attached channels.
- Free SHM slots promptly on cancellation.
- Handle `CancelChannel` idempotently.

## Deadlines

- Check deadlines before sending requests when possible.
- When canceling server-side, send error responses when possible and drain pending writes gracefully.
- After `CancelChannel`, ignore further data frames for that channel; consider closing the connection if a peer repeatedly violates protocol expectations.
