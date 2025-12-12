+++
title = "Motivation"
description = "Why rapace exists and how dodeca uses it"
+++

rapace was originally written for one very specific job: letting [dodeca](https://dodeca.bearcove.eu/) talk to plugins as separate processes, without forcing everything into a single, ever-growing binary.

## Background: dodeca and plugins

Dodeca does a lot of work that pulls in heavy dependencies: HTML and CSS processing, image encoding and transformation, syntax highlighting, and so on. If all of that lives inside the main `ddc` binary, you end up with a large dependency tree and slow final link times, especially on macOS where the linker is not particularly fast. Even with incremental builds, the last link step still hurts.

The idea behind rapace was to move that work out into separate binaries. Instead of one binary that knows how to do everything, you keep a smaller host and let individual plugins handle the complex pieces. The host and the plugins then need a way to talk to each other.

## Why not just dynamic libraries?

A first attempt used dynamic libraries. That works to a point, but sharing an address space with plugins brings its own problems. Unloading or replacing a plugin is hard to do safely if the host and plugin are mixed together in one process. You also end up thinking in terms of exported symbols and ABI details rather than messages.

Separate executables with a clear boundary are simpler in some ways. A plugin can crash without taking down the host. In principle you can stop one process and start another in its place. The communication story becomes "send a request, get a response" instead of "call into this shared object and hope all the state is in a good place".

That pushed the design towards a message‑based boundary instead of dynamic linking.

## From messages to shared memory

The next obvious step was to pass structured messages between host and plugin. Control data can be serialized just fine (for example with [postcard](https://postcard.jamesmunns.com/)), and the two sides can agree on a small RPC interface. For many calls that is enough.

Some workloads, however, deal in large blobs of data. Image encoding is a good example: you do not really want to serialize and copy large pixel buffers back and forth if you can avoid it. For that kind of traffic, plain message passing starts to look inefficient.

rapace’s shared‑memory transport is a response to that. The control path still uses normal RPC messages, but large buffers can live in a shared memory region that both processes can see. The transport code knows how to ship references to those regions so that the receiver can effectively "borrow" the data without an extra copy, while the host and plugin stay in separate processes.

The same RPC abstraction sits on top of all transports, so code written against a [`#[rapace::service]`](https://docs.rs/rapace-macros/latest/rapace_macros/attr.service.html) trait does not have to care whether the underlying path is shared memory, a WebSocket, a TCP‑like stream, or an in‑memory channel used for tests.

## Tracing over rapace

There is a companion crate, [`rapace-tracing`](https://docs.rs/rapace-tracing), which lets a plugin install a tracing subscriber that forwards spans and events back to the host over rapace. Plugin code can use the usual `tracing` macros, and the host can receive those events and log or display them.

Dodeca uses this so that plugin tracing is forwarded back to the host process, which keeps behaviour across host and plugins visible in one place without a separate logging protocol.

## How dodeca uses rapace today

In dodeca, rapace currently sits between the main `ddc` process and some of its plugins. Syntax highlighting is one example of functionality that runs in a separate process and talks back to the host over rapace. Other plugin‑style features can use the same path.

Because plugins are separate executables, it is at least conceptually possible to rebuild one, stop the old process, and start a new one that reconnects over rapace. Doing the equivalent with dynamic libraries in a shared address space is much harder, because you have to be sure that all state associated with the old code has been torn down before anything is unloaded.

The same RPC model is also used in other places around the tooling. For example, the WebSocket transport lets a development server talk to tools running in a browser, using the same service definition mechanism as the host↔plugin link.

At the transport layer, rapace provides:

- a shared‑memory transport, used between host and plugins in dodeca;
- a WebSocket transport, used for browser‑based tools;
- an in‑memory transport, mainly for tests and small experiments;
- and a stream transport (TCP/Unix‑style), which exists but is not currently used in this setup.

The goal of this page is simply to record how rapace is used inside dodeca and why it was shaped the way it is, not to claim particular performance characteristics or to position it as a general‑purpose RPC framework.
