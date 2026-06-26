//! Tokio task-local query frames used for dependency recording and cycle detection.

use crate::key::{Dep, DynKey};
use crate::revision::Revision;
use std::cell::RefCell;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::trace;

// r[frame.task-local]
// r[frame.cycle-stack]
tokio::task_local! {
    static ACTIVE_STACK: RefCell<ActiveStack>;
}

static NEXT_FRAME_ID: AtomicU64 = AtomicU64::new(1);

// r[frame.purpose]
/// A cheap, clonable handle for the currently-running query frame.
#[derive(Clone)]
pub struct ActiveFrameHandle {
    id: u64,
    dyn_key: DynKey,
    started_at: Revision,
}

struct ActiveStack {
    frames: Vec<ActiveFrame>,
}

// r[frame.no-lock-await]
struct ActiveFrame {
    id: u64,
    dyn_key: DynKey,
    deps: Vec<Dep>,
}

impl ActiveFrameHandle {
    /// Create a new frame for `dyn_key`, recording dependencies at `started_at`.
    pub fn new(dyn_key: DynKey, started_at: Revision) -> Self {
        Self {
            id: NEXT_FRAME_ID.fetch_add(1, Ordering::Relaxed),
            dyn_key,
            started_at,
        }
    }

    /// The erased key for this frame.
    pub fn dyn_key(&self) -> &DynKey {
        &self.dyn_key
    }

    /// The revision at which the frame started.
    pub fn started_at(&self) -> Revision {
        self.started_at
    }

    /// Drain the recorded dependency list.
    pub fn take_deps(&self) -> Vec<Dep> {
        ACTIVE_STACK
            .try_with(|stack| {
                let mut stack = stack.borrow_mut();
                stack
                    .frames
                    .iter_mut()
                    .find(|frame| frame.id == self.id)
                    .map(|frame| std::mem::take(&mut frame.deps))
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }
}

/// Guard that pops the active frame when dropped.
pub struct FrameGuard {
    id: u64,
    popped: bool,
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        if self.popped {
            return;
        }
        let _ = ACTIVE_STACK.try_with(|stack| {
            let mut stack = stack.borrow_mut();
            let popped = if stack.frames.last().is_some_and(|frame| frame.id == self.id) {
                stack.frames.pop()
            } else {
                stack
                    .frames
                    .iter()
                    .rposition(|frame| frame.id == self.id)
                    .map(|index| stack.frames.remove(index))
            };
            trace!(popped = popped.is_some(), "pop_frame");
        });
        self.popped = true;
    }
}

/// Run `f` with a task-local query stack, creating one if needed.
pub async fn scope_if_needed<F, Fut, R>(f: F) -> R
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = R>,
{
    if ACTIVE_STACK.try_with(|_| ()).is_ok() {
        f().await
    } else {
        ACTIVE_STACK
            .scope(RefCell::new(ActiveStack { frames: Vec::new() }), f())
            .await
    }
}

/// Run a boxed future with a task-local query stack, creating one if needed.
///
/// This variant accepts a pre-boxed future to avoid monomorphization overhead.
/// Use this instead of [`scope_if_needed`] in generic contexts where each call
/// site would otherwise create a unique monomorphization.
pub async fn scope_if_needed_boxed<R>(
    fut: std::pin::Pin<Box<dyn Future<Output = R> + Send + '_>>,
) -> R {
    if ACTIVE_STACK.try_with(|_| ()).is_ok() {
        fut.await
    } else {
        ACTIVE_STACK
            .scope(RefCell::new(ActiveStack { frames: Vec::new() }), fut)
            .await
    }
}

/// Returns `true` if there is a current query frame.
pub fn has_active_frame() -> bool {
    ACTIVE_STACK
        .try_with(|stack| !stack.borrow().frames.is_empty())
        .unwrap_or(false)
}

// r[frame.record-dep]
// r[dep.recording]
/// Record a dependency on the current top-of-stack frame, if any.
pub fn record_dep(dep: Dep) {
    let _ = record_dep_if_active(dep);
}

pub(crate) fn record_dep_if_active(dep: Dep) -> bool {
    ACTIVE_STACK
        .try_with(|stack| {
            let mut stack = stack.borrow_mut();
            let Some(top) = stack.frames.last_mut() else {
                return false;
            };

            top.deps.push(dep);
            true
        })
        .unwrap_or(false)
}

pub(crate) fn record_dep_with(make_dep: impl FnOnce() -> Dep) -> bool {
    ACTIVE_STACK
        .try_with(|stack| {
            let mut stack = stack.borrow_mut();
            let Some(top) = stack.frames.last_mut() else {
                return false;
            };

            top.deps.push(make_dep());
            true
        })
        .unwrap_or(false)
}

pub(crate) fn record_dep_result<E>(make_dep: impl FnOnce() -> Result<Dep, E>) -> Result<bool, E> {
    ACTIVE_STACK
        .try_with(|stack| {
            let mut stack = stack.borrow_mut();
            let Some(top) = stack.frames.last_mut() else {
                return Ok(false);
            };

            top.deps.push(make_dep()?);
            Ok(true)
        })
        .unwrap_or(Ok(false))
}

// r[frame.cycle-detect]
// r[frame.cycle-per-task]
/// If `requested` already exists in the task-local stack, returns the full stack of `DynKey`s.
pub fn find_cycle(requested: &DynKey) -> Option<Vec<DynKey>> {
    ACTIVE_STACK
        .try_with(|stack| {
            let stack = stack.borrow();
            let has_cycle = stack.frames.iter().any(|f| &f.dyn_key == requested);
            if !has_cycle {
                return None;
            }
            Some(stack.frames.iter().map(|f| f.dyn_key.clone()).collect())
        })
        .ok()
        .flatten()
}

/// Push a frame onto the task-local stack. Requires an active scope (see [`scope_if_needed`]).
pub fn push_frame(frame: ActiveFrameHandle) -> FrameGuard {
    let id = frame.id;
    let _ = ACTIVE_STACK.try_with(|stack| {
        let mut stack = stack.borrow_mut();
        stack.frames.push(ActiveFrame {
            id,
            dyn_key: frame.dyn_key.clone(),
            deps: Vec::new(),
        });
    });
    FrameGuard { id, popped: false }
}
