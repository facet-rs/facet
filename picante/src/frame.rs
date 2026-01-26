//! Tokio task-local query frames used for dependency recording and cycle detection.

use crate::key::{Dep, DynKey};
use crate::revision::Revision;
use parking_lot::Mutex;
use std::cell::RefCell;
use std::future::Future;
use std::sync::Arc;
use tracing::trace;

// r[frame.task-local]
// r[frame.cycle-stack]
tokio::task_local! {
    static ACTIVE_STACK: RefCell<Vec<ActiveFrameHandle>>;
}

// r[frame.purpose]
/// A cheap, clonable handle for the currently-running query frame.
#[derive(Clone)]
pub struct ActiveFrameHandle(Arc<ActiveFrameInner>);

// r[frame.no-lock-await]
struct ActiveFrameInner {
    dyn_key: DynKey,
    started_at: Revision,
    deps: Mutex<Vec<Dep>>,
}

impl ActiveFrameHandle {
    /// Create a new frame for `dyn_key`, recording dependencies at `started_at`.
    pub fn new(dyn_key: DynKey, started_at: Revision) -> Self {
        Self(Arc::new(ActiveFrameInner {
            dyn_key,
            started_at,
            deps: Mutex::new(Vec::new()),
        }))
    }

    /// The erased key for this frame.
    pub fn dyn_key(&self) -> &DynKey {
        &self.0.dyn_key
    }

    /// The revision at which the frame started.
    pub fn started_at(&self) -> Revision {
        self.0.started_at
    }

    /// Drain the recorded dependency list.
    pub fn take_deps(&self) -> Vec<Dep> {
        let mut deps = self.0.deps.lock();
        std::mem::take(&mut *deps)
    }
}

/// Guard that pops the active frame when dropped.
pub struct FrameGuard {
    popped: bool,
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        if self.popped {
            return;
        }
        let _ = ACTIVE_STACK.try_with(|stack| {
            let popped = stack.borrow_mut().pop();
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
        ACTIVE_STACK.scope(RefCell::new(Vec::new()), f()).await
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
        ACTIVE_STACK.scope(RefCell::new(Vec::new()), fut).await
    }
}

/// Returns `true` if there is a current query frame.
pub fn has_active_frame() -> bool {
    ACTIVE_STACK
        .try_with(|stack| !stack.borrow().is_empty())
        .unwrap_or(false)
}

// r[frame.record-dep]
// r[dep.recording]
/// Record a dependency on the current top-of-stack frame, if any.
pub fn record_dep(dep: Dep) {
    let _ = ACTIVE_STACK.try_with(|stack| {
        if let Some(top) = stack.borrow().last() {
            top.0.deps.lock().push(dep);
        }
    });
}

// r[frame.cycle-detect]
// r[frame.cycle-per-task]
/// If `requested` already exists in the task-local stack, returns the full stack of `DynKey`s.
pub fn find_cycle(requested: &DynKey) -> Option<Vec<DynKey>> {
    ACTIVE_STACK
        .try_with(|stack| {
            let stack = stack.borrow();
            let has_cycle = stack.iter().any(|f| f.dyn_key() == requested);
            if !has_cycle {
                return None;
            }
            Some(stack.iter().map(|f| f.dyn_key().clone()).collect())
        })
        .ok()
        .flatten()
}

/// Push a frame onto the task-local stack. Requires an active scope (see [`scope_if_needed`]).
pub fn push_frame(frame: ActiveFrameHandle) -> FrameGuard {
    let _ = ACTIVE_STACK.try_with(|stack| {
        stack.borrow_mut().push(frame);
    });
    FrameGuard { popped: false }
}
