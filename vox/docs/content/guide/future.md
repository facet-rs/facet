+++
title = "Future directions"
description = "Ideas rapace might enable later"
+++

This page is about possible uses of rapace that do not exist yet, but are interesting to think about.

## Plugin reloads

Today, dodeca starts its plugins once at launch and they run until the process exits. Because plugins are separate executables that talk to the host over rapace instead of being dynamically loaded into the same address space, it is at least conceptually possible to do more:

- rebuild a plugin binary;
- stop the old process;
- start the new one and reconnect over the same RPC surface.

Getting that right involves details that are not implemented today (how to hand off in‑flight requests, how to coordinate state between old and new instances, etc.), but the "host + external plugin over IPC/RPC" design makes the problem more tractable than shared‑library hot‑reloading inside a single process.

## Mutation at a distance

Another idea is to treat rapace as a way to mutate state at a distance in a controlled way. Instead of always sending full values back and forth, you could imagine:

- keeping some piece of state on one side (a host, a plugin, or a remote service);
- describing changes as diffs or small operations;
- applying those diffs over a rapace channel, possibly across different transports (SHM, WebSocket, stream).

The existing pieces (service traits, facet‑based schemas, postcard encoding, frames, channels) are all oriented around sending typed messages. A future layer could interpret some of those messages as patches or transactional updates to long‑lived objects, whether they live in the same machine or on the other end of a network connection.

## Code generation for other languages

Even though rapace is very Rust‑centric internally, the information you’d need to talk to it from other languages already exists: service traits, facet‑derived shapes for request/response types, and the registry that ties method IDs to those shapes.

One possible direction is to treat a rapace service crate as a build‑time dependency from some other project (for example a Svelte or React frontend, or a non‑Rust backend), and run a code generator that:

- loads the Rust service definitions and registry;
- walks the facet shapes for each request/response type;
- emits client code in another language that speaks the same postcard‑encoded protocol over a chosen transport (WebSocket, stream, etc.).

The main missing piece today is a stable way to reflect over traits, methods, and their argument/return types at build time. The type‑level part is already in place via facet; the function/method‑level part would need some extra machinery. In principle, though, nothing about rapace requires the client to be written in Rust, as long as it can match the framing rules and the facet/postcard encoding.

None of this is designed or implemented yet; this page just sketches the kind of directions rapace was written to leave open.