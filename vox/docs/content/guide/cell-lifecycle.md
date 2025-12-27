+++
title = "Cell Lifecycle"
description = "Detecting cell death and automatic relaunching"
+++

This page describes how to detect when a cell process dies and how to implement automatic relaunching. This is host-side functionality for managing the lifecycle of cells in a hub-based architecture.

## Overview

In a hub-based setup, a host process spawns multiple cell processes. Each cell connects to the hub via shared memory and communicates through a doorbell (a Unix socketpair used for wakeups).

When a cell crashes or exits unexpectedly:

1. The doorbell socket becomes broken (the peer end is closed)
2. Subsequent `signal()` calls fail with `EPIPE`, `ECONNRESET`, or `ENOTCONN`
3. The host-side transport detects this and can notify the application

rapace provides infrastructure for detecting peer death and invoking callbacks, but the actual respawning logic lives in your application.

## Peer Death Detection

The doorbell's `signal()` method returns a `SignalResult`:

```rust,noexec
pub enum SignalResult {
    /// Signal was sent successfully.
    Sent,
    /// Buffer was full but peer is alive (signal coalesced with pending ones).
    BufferFull,
    /// Peer has disconnected (socket broken).
    PeerDead,
}
```

When `PeerDead` is returned:

- The transport logs a warning **once** (no spam)
- The transport is marked as closed
- Any configured death callback is invoked

## Setting Up Death Callbacks

Use `AddPeerOptions` when adding peers to a hub:

```rust,noexec
use std::sync::Arc;
use rapace::transport::shm::{HubHost, HubConfig, AddPeerOptions};

let hub = Arc::new(HubHost::create("/tmp/my-hub.shm", HubConfig::default())?);

// Channel to receive death notifications
let (death_tx, death_rx) = tokio::sync::mpsc::unbounded_channel();

let (transport, ticket) = hub.add_peer_transport_with_options(AddPeerOptions {
    // Human-readable name for logging
    peer_name: Some("my-cell".into()),

    // Callback invoked when peer dies
    on_death: Some(Arc::new(move |peer_id| {
        let _ = death_tx.send(peer_id);
    })),
})?;
```

The callback receives the `peer_id` of the dead peer. Since it may be called from an async context, it should be non-blocking (use channels, atomics, or spawn a task).

## Implementing Automatic Relaunch

To automatically relaunch dead cells, you need to:

1. **Track spawn information** – remember how each cell was spawned
2. **Listen for death events** – handle the callback
3. **Respawn the cell** – create a new peer slot and spawn a new process

Here's a complete example:

```rust,noexec
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use rapace::transport::shm::{HubHost, HubConfig, AddPeerOptions, HubPeerTicket};
use tokio::sync::mpsc;

/// Information needed to respawn a cell.
struct CellRecipe {
    binary: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

/// Manages cell lifecycle with automatic relaunch.
struct CellManager {
    hub: Arc<HubHost>,
    /// Maps peer_id -> spawn recipe
    recipes: Mutex<HashMap<u16, CellRecipe>>,
    /// Channel for death notifications
    death_tx: mpsc::UnboundedSender<u16>,
}

impl CellManager {
    fn new(hub: Arc<HubHost>) -> (Arc<Self>, mpsc::UnboundedReceiver<u16>) {
        let (death_tx, death_rx) = mpsc::unbounded_channel();
        let manager = Arc::new(Self {
            hub,
            recipes: Mutex::new(HashMap::new()),
            death_tx,
        });
        (manager, death_rx)
    }

    /// Spawn a new cell with automatic relaunch on death.
    fn spawn_cell(
        self: &Arc<Self>,
        binary: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> std::io::Result<u16> {
        let death_tx = self.death_tx.clone();

        // Create peer slot with death callback
        let (transport, ticket) = self.hub.add_peer_transport_with_options(AddPeerOptions {
            peer_name: Some(binary.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "cell".into())),
            on_death: Some(Arc::new(move |peer_id| {
                let _ = death_tx.send(peer_id);
            })),
        }).map_err(|e| std::io::Error::other(e.to_string()))?;

        let peer_id = ticket.peer_id;

        // Store the recipe for potential relaunch
        {
            let mut recipes = self.recipes.lock().unwrap();
            recipes.insert(peer_id, CellRecipe {
                binary: binary.clone(),
                args: args.clone(),
                env: env.clone(),
            });
        }

        // Spawn the cell process
        self.spawn_process(&binary, &args, &env, &ticket)?;

        // The ticket is dropped here, closing the peer FD on the host side
        // (the child inherited it before exec)

        Ok(peer_id)
    }

    fn spawn_process(
        &self,
        binary: &PathBuf,
        args: &[String],
        env: &[(String, String)],
        ticket: &HubPeerTicket,
    ) -> std::io::Result<()> {
        let mut cmd = Command::new(binary);
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Add hub connection arguments
        ticket.apply_to_command(&mut cmd);

        // Spawn (you may want to store the Child handle)
        cmd.spawn()?;
        Ok(())
    }

    /// Handle a death notification by respawning the cell.
    fn handle_death(&self, peer_id: u16) -> std::io::Result<u16> {
        let recipe = {
            let mut recipes = self.recipes.lock().unwrap();
            recipes.remove(&peer_id)
        };

        match recipe {
            Some(recipe) => {
                tracing::info!(peer_id, binary = ?recipe.binary, "Relaunching dead cell");
                // Note: This creates a NEW peer_id. The old one is gone.
                // If you need to maintain peer_id stability, you'd need
                // additional hub-level support.
                self.spawn_cell(recipe.binary, recipe.args, recipe.env)
            }
            None => {
                tracing::warn!(peer_id, "No recipe found for dead cell, cannot relaunch");
                Err(std::io::Error::other("no recipe for peer"))
            }
        }
    }
}

// Main loop that handles death notifications
async fn run_manager(manager: Arc<CellManager>, mut death_rx: mpsc::UnboundedReceiver<u16>) {
    while let Some(peer_id) = death_rx.recv().await {
        match manager.handle_death(peer_id) {
            Ok(new_peer_id) => {
                tracing::info!(old_peer_id = peer_id, new_peer_id, "Cell relaunched");
            }
            Err(e) => {
                tracing::error!(peer_id, error = %e, "Failed to relaunch cell");
            }
        }
    }
}
```

## Considerations

### Relaunch Limits

You may want to limit how many times a cell can be relaunched to prevent crash loops:

```rust,noexec
struct CellRecipe {
    binary: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    relaunch_count: u32,
    max_relaunches: u32,
}

fn handle_death(&self, peer_id: u16) -> std::io::Result<u16> {
    let mut recipe = /* get recipe */;

    if recipe.relaunch_count >= recipe.max_relaunches {
        tracing::error!(peer_id, "Cell exceeded max relaunches, giving up");
        return Err(std::io::Error::other("max relaunches exceeded"));
    }

    recipe.relaunch_count += 1;
    // ... spawn
}
```

### Backoff

Consider adding exponential backoff between relaunches:

```rust,noexec
let delay = Duration::from_millis(100 * 2u64.pow(recipe.relaunch_count.min(10)));
tokio::time::sleep(delay).await;
```

### Peer ID Stability

When a cell is relaunched, it gets a **new** `peer_id`. If your application logic depends on stable peer IDs, you'll need to maintain a mapping:

```rust,noexec
/// Maps logical cell name -> current peer_id
cell_ids: Mutex<HashMap<String, u16>>,
```

### Graceful Shutdown vs Crash

The death callback is invoked whenever the doorbell fails, which happens both for crashes and graceful shutdowns. If you need to distinguish:

- Have cells send an explicit "shutting down" message before exiting
- Track which cells are in "shutting down" state
- Only relaunch cells that didn't announce their shutdown

## Logging

When a cell dies, rapace logs a single warning with the peer information:

```
WARN peer_id=3 peer_name="my-cell" Cell died (doorbell signal failed)
```

This replaces the previous behavior that would spam `doorbell signal failed` for every subsequent signal attempt.

## API Reference

### `SignalResult`

```rust,noexec
pub enum SignalResult {
    Sent,       // Signal delivered
    BufferFull, // Buffer full, peer alive (coalesced)
    PeerDead,   // Peer disconnected
}
```

### `AddPeerOptions`

```rust,noexec
pub struct AddPeerOptions {
    /// Human-readable name for logging (defaults to "peer-{id}")
    pub peer_name: Option<String>,

    /// Callback invoked when peer dies
    pub on_death: Option<PeerDeathCallback>,
}

pub type PeerDeathCallback = Arc<dyn Fn(u16) + Send + Sync + 'static>;
```

### `HubHost::add_peer_transport_with_options`

```rust,noexec
impl HubHost {
    pub fn add_peer_transport_with_options(
        self: &Arc<Self>,
        options: AddPeerOptions,
    ) -> Result<(AnyTransport, HubPeerTicket), HubSessionError>;
}
```
