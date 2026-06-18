// r[impl rpc.channel.delivery.reliable] r[impl rpc.flow-control.credit]

#[cfg(not(target_arch = "wasm32"))]
pub mod mpsc {
    use std::fmt;

    pub use tokio::sync::mpsc::error::{SendError, TryRecvError, TrySendError};
    pub use tokio::sync::mpsc::{OwnedPermit, Receiver, Sender, error};

    pub fn channel<T>(_name: impl Into<String>, capacity: usize) -> (Sender<T>, Receiver<T>) {
        tokio::sync::mpsc::channel(capacity)
    }

    pub struct UnboundedSender<T>(tokio::sync::mpsc::UnboundedSender<T>);

    impl<T> Clone for UnboundedSender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> fmt::Debug for UnboundedSender<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<T> UnboundedSender<T> {
        pub fn send(&self, message: T) -> Result<(), SendError<T>> {
            self.0.send(message)
        }

        pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
            self.0
                .send(message)
                .map_err(|error| TrySendError::Closed(error.0))
        }

        pub fn is_closed(&self) -> bool {
            self.0.is_closed()
        }
    }

    pub struct UnboundedReceiver<T>(tokio::sync::mpsc::UnboundedReceiver<T>);

    impl<T> fmt::Debug for UnboundedReceiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<T> UnboundedReceiver<T> {
        pub async fn recv(&mut self) -> Option<T> {
            self.0.recv().await
        }

        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv()
        }

        pub fn close(&mut self) {
            self.0.close()
        }
    }

    pub fn unbounded_channel<T>(
        _name: impl Into<String>,
    ) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (UnboundedSender(tx), UnboundedReceiver(rx))
    }
}

#[cfg(target_arch = "wasm32")]
pub mod mpsc {
    use std::{
        cell::RefCell,
        collections::VecDeque,
        fmt,
        future::Future,
        pin::Pin,
        rc::Rc,
        task::{Context, Poll, Waker},
    };

    pub mod error {
        pub use super::{SendError, TryRecvError, TrySendError};
    }

    #[derive(Debug, PartialEq, Eq)]
    pub struct SendError<T>(pub T);

    #[derive(Debug, PartialEq, Eq)]
    pub enum TrySendError<T> {
        Full(T),
        Closed(T),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TryRecvError {
        Empty,
        Disconnected,
    }

    impl<T> fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("send failed because channel is closed")
        }
    }

    impl<T: fmt::Debug> std::error::Error for SendError<T> {}

    impl<T> fmt::Display for TrySendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Full(_) => f.write_str("send failed because channel is full"),
                Self::Closed(_) => f.write_str("send failed because channel is closed"),
            }
        }
    }

    impl<T: fmt::Debug> std::error::Error for TrySendError<T> {}

    impl fmt::Display for TryRecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Empty => f.write_str("receiver channel is empty"),
                Self::Disconnected => f.write_str("receiver channel is disconnected"),
            }
        }
    }

    impl std::error::Error for TryRecvError {}

    struct Shared<T> {
        queue: VecDeque<T>,
        capacity: usize,
        reserved: usize,
        closed: bool,
        sender_count: usize,
        recv_wakers: Vec<Waker>,
        send_wakers: Vec<Waker>,
    }

    impl<T> Shared<T> {
        fn available(&self) -> usize {
            self.capacity
                .saturating_sub(self.queue.len().saturating_add(self.reserved))
        }

        fn wake_receivers(&mut self) {
            for waker in self.recv_wakers.drain(..) {
                waker.wake();
            }
        }

        fn wake_senders(&mut self) {
            for waker in self.send_wakers.drain(..) {
                waker.wake();
            }
        }
    }

    pub struct Sender<T> {
        shared: Rc<RefCell<Shared<T>>>,
    }

    pub struct Receiver<T> {
        shared: Rc<RefCell<Shared<T>>>,
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            self.shared.borrow_mut().sender_count += 1;
            Self {
                shared: self.shared.clone(),
            }
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            let mut shared = self.shared.borrow_mut();
            shared.sender_count = shared.sender_count.saturating_sub(1);
            if shared.sender_count == 0 {
                shared.wake_receivers();
            }
        }
    }

    impl<T> fmt::Debug for Sender<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let shared = self.shared.borrow();
            f.debug_struct("Sender")
                .field("closed", &shared.closed)
                .field("len", &shared.queue.len())
                .field("capacity", &shared.capacity)
                .finish()
        }
    }

    impl<T> fmt::Debug for Receiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let shared = self.shared.borrow();
            f.debug_struct("Receiver")
                .field("closed", &shared.closed)
                .field("len", &shared.queue.len())
                .field("capacity", &shared.capacity)
                .finish()
        }
    }

    pub fn channel<T>(_name: impl Into<String>, capacity: usize) -> (Sender<T>, Receiver<T>) {
        let queue = if capacity == usize::MAX {
            VecDeque::new()
        } else {
            VecDeque::with_capacity(capacity)
        };
        let shared = Rc::new(RefCell::new(Shared {
            queue,
            capacity,
            reserved: 0,
            closed: false,
            sender_count: 1,
            recv_wakers: Vec::new(),
            send_wakers: Vec::new(),
        }));
        (
            Sender {
                shared: shared.clone(),
            },
            Receiver { shared },
        )
    }

    impl<T> Sender<T> {
        pub fn send(&self, message: T) -> SendFuture<T> {
            SendFuture {
                shared: self.shared.clone(),
                item: Some(message),
            }
        }

        pub fn reserve_owned(self) -> ReserveOwnedFuture<T> {
            ReserveOwnedFuture { sender: Some(self) }
        }

        pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
            let mut shared = self.shared.borrow_mut();
            if shared.closed {
                return Err(TrySendError::Closed(message));
            }
            if shared.available() == 0 {
                return Err(TrySendError::Full(message));
            }
            shared.queue.push_back(message);
            shared.wake_receivers();
            Ok(())
        }

        pub fn is_closed(&self) -> bool {
            self.shared.borrow().closed
        }

        pub fn max_capacity(&self) -> usize {
            self.shared.borrow().capacity
        }

        pub fn capacity(&self) -> usize {
            self.shared.borrow().available()
        }
    }

    pub struct SendFuture<T> {
        shared: Rc<RefCell<Shared<T>>>,
        item: Option<T>,
    }

    impl<T> Future for SendFuture<T> {
        type Output = Result<(), SendError<T>>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = unsafe { self.get_unchecked_mut() };
            let item = this
                .item
                .take()
                .expect("send future polled after completion");
            let mut shared = this.shared.borrow_mut();
            if shared.closed {
                return Poll::Ready(Err(SendError(item)));
            }
            if shared.available() == 0 {
                this.item = Some(item);
                shared.send_wakers.push(cx.waker().clone());
                return Poll::Pending;
            }
            shared.queue.push_back(item);
            shared.wake_receivers();
            Poll::Ready(Ok(()))
        }
    }

    pub struct ReserveOwnedFuture<T> {
        sender: Option<Sender<T>>,
    }

    impl<T> Future for ReserveOwnedFuture<T> {
        type Output = Result<OwnedPermit<T>, SendError<()>>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = unsafe { self.get_unchecked_mut() };
            let sender = this
                .sender
                .as_ref()
                .expect("reserve future polled after completion");
            let (closed, available) = {
                let shared = sender.shared.borrow();
                (shared.closed, shared.available())
            };
            if closed {
                this.sender.take();
                return Poll::Ready(Err(SendError(())));
            }
            if available == 0 {
                let mut shared = sender.shared.borrow_mut();
                shared.send_wakers.push(cx.waker().clone());
                return Poll::Pending;
            }
            let mut shared = sender.shared.borrow_mut();
            shared.reserved += 1;
            drop(shared);
            Poll::Ready(Ok(OwnedPermit {
                sender: this.sender.take(),
            }))
        }
    }

    pub struct OwnedPermit<T> {
        sender: Option<Sender<T>>,
    }

    impl<T> OwnedPermit<T> {
        pub fn send(mut self, item: T) -> Sender<T> {
            let sender = self.sender.take().expect("permit used after completion");
            let mut shared = sender.shared.borrow_mut();
            shared.reserved = shared.reserved.saturating_sub(1);
            if !shared.closed {
                shared.queue.push_back(item);
                shared.wake_receivers();
            }
            drop(shared);
            sender
        }
    }

    impl<T> Drop for OwnedPermit<T> {
        fn drop(&mut self) {
            if let Some(sender) = self.sender.take() {
                let mut shared = sender.shared.borrow_mut();
                shared.reserved = shared.reserved.saturating_sub(1);
                shared.wake_senders();
            }
        }
    }

    impl<T> Receiver<T> {
        pub fn recv(&mut self) -> RecvFuture<T> {
            RecvFuture {
                shared: self.shared.clone(),
            }
        }

        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            let mut shared = self.shared.borrow_mut();
            if let Some(item) = shared.queue.pop_front() {
                shared.wake_senders();
                return Ok(item);
            }
            if shared.closed || shared.sender_count == 0 {
                return Err(TryRecvError::Disconnected);
            }
            Err(TryRecvError::Empty)
        }

        pub fn close(&mut self) {
            let mut shared = self.shared.borrow_mut();
            shared.closed = true;
            shared.wake_senders();
        }
    }

    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            self.close();
        }
    }

    pub struct RecvFuture<T> {
        shared: Rc<RefCell<Shared<T>>>,
    }

    impl<T> Future for RecvFuture<T> {
        type Output = Option<T>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut shared = self.shared.borrow_mut();
            if let Some(item) = shared.queue.pop_front() {
                shared.wake_senders();
                return Poll::Ready(Some(item));
            }
            if shared.closed || shared.sender_count == 0 {
                return Poll::Ready(None);
            }
            shared.recv_wakers.push(cx.waker().clone());
            Poll::Pending
        }
    }

    pub struct UnboundedSender<T>(Sender<T>);

    impl<T> Clone for UnboundedSender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> fmt::Debug for UnboundedSender<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<T> UnboundedSender<T> {
        pub fn send(&self, message: T) -> Result<(), SendError<T>> {
            self.0.try_send(message).map_err(|error| match error {
                TrySendError::Full(message) | TrySendError::Closed(message) => SendError(message),
            })
        }

        pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
            self.0.try_send(message)
        }

        pub fn is_closed(&self) -> bool {
            self.0.is_closed()
        }
    }

    pub struct UnboundedReceiver<T>(Receiver<T>);

    impl<T> fmt::Debug for UnboundedReceiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<T> UnboundedReceiver<T> {
        pub async fn recv(&mut self) -> Option<T> {
            self.0.recv().await
        }

        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv()
        }

        pub fn close(&mut self) {
            self.0.close()
        }
    }

    pub fn unbounded_channel<T>(
        _name: impl Into<String>,
    ) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let (tx, rx) = channel(_name, usize::MAX);
        (UnboundedSender(tx), UnboundedReceiver(rx))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod oneshot {
    pub use tokio::sync::oneshot::{Receiver, Sender, error};

    pub fn channel<T>(_name: impl Into<String>) -> (Sender<T>, Receiver<T>) {
        tokio::sync::oneshot::channel()
    }
}

#[cfg(target_arch = "wasm32")]
pub mod oneshot {
    pub mod error {
        pub use futures_channel::oneshot::Canceled as RecvError;
    }

    pub use futures_channel::oneshot::{Receiver, Sender};

    pub fn channel<T>(_name: impl Into<String>) -> (Sender<T>, Receiver<T>) {
        futures_channel::oneshot::channel()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod mutex {
    #![expect(
        clippy::disallowed_types,
        reason = "vox-rt is the facade that wraps Tokio mutexes"
    )]

    use std::fmt;

    pub struct Mutex<T>(tokio::sync::Mutex<T>);

    pub use tokio::sync::MutexGuard;

    impl<T> Mutex<T> {
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(tokio::sync::Mutex::new(value))
        }

        pub async fn lock(&self) -> MutexGuard<'_, T> {
            self.0.lock().await
        }

        pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_lock()
        }
    }

    impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod sync_mutex {
        use std::{fmt, ops::Deref};

        pub struct SyncMutex<T>(parking_lot::Mutex<T>);

        pub use parking_lot::MutexGuard as SyncMutexGuard;

        impl<T> SyncMutex<T> {
            pub fn new(_name: &'static str, value: T) -> Self {
                Self(parking_lot::Mutex::new(value))
            }

            pub fn lock(&self) -> SyncMutexGuard<'_, T> {
                self.0.lock()
            }

            pub fn try_lock(&self) -> Option<SyncMutexGuard<'_, T>> {
                self.0.try_lock()
            }
        }

        impl<T> Deref for SyncMutex<T> {
            type Target = parking_lot::Mutex<T>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<T: fmt::Debug> fmt::Debug for SyncMutex<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    mod sync_mutex {
        use std::fmt;

        pub struct SyncMutex<T>(std::sync::Mutex<T>);

        pub type SyncMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;

        impl<T> SyncMutex<T> {
            pub fn new(_name: &'static str, value: T) -> Self {
                Self(std::sync::Mutex::new(value))
            }

            pub fn lock(&self) -> SyncMutexGuard<'_, T> {
                self.0.lock().expect("vox runtime mutex poisoned")
            }

            pub fn try_lock(&self) -> Option<SyncMutexGuard<'_, T>> {
                match self.0.try_lock() {
                    Ok(guard) => Some(guard),
                    Err(std::sync::TryLockError::WouldBlock) => None,
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        panic!("vox runtime mutex poisoned")
                    }
                }
            }
        }

        impl<T: fmt::Debug> fmt::Debug for SyncMutex<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    }

    pub use sync_mutex::*;
}

#[cfg(target_arch = "wasm32")]
mod mutex {
    use std::fmt;

    pub struct Mutex<T>(std::sync::Mutex<T>);

    pub type MutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;

    #[derive(Debug)]
    pub struct TryLockError;

    impl<T> Mutex<T> {
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(std::sync::Mutex::new(value))
        }

        pub async fn lock(&self) -> MutexGuard<'_, T> {
            self.0.lock().expect("vox runtime mutex poisoned")
        }

        pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, TryLockError> {
            self.0.try_lock().map_err(|_| TryLockError)
        }
    }

    impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    pub struct SyncMutex<T>(std::sync::Mutex<T>);

    pub type SyncMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;

    impl<T> SyncMutex<T> {
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(std::sync::Mutex::new(value))
        }

        pub fn lock(&self) -> SyncMutexGuard<'_, T> {
            self.0.lock().expect("vox runtime mutex poisoned")
        }

        pub fn try_lock(&self) -> Option<SyncMutexGuard<'_, T>> {
            match self.0.try_lock() {
                Ok(guard) => Some(guard),
                Err(std::sync::TryLockError::WouldBlock) => None,
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    panic!("vox runtime mutex poisoned")
                }
            }
        }
    }

    impl<T: fmt::Debug> fmt::Debug for SyncMutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
}

pub use mutex::*;

#[cfg(not(target_arch = "wasm32"))]
mod notify {
    use std::fmt;
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct Notify(Arc<tokio::sync::Notify>);

    impl Notify {
        pub fn new(_name: impl Into<String>) -> Self {
            Self(Arc::new(tokio::sync::Notify::new()))
        }

        pub async fn notified(&self) {
            self.0.notified().await
        }

        pub fn notify_one(&self) {
            self.0.notify_one()
        }

        pub fn notify_waiters(&self) {
            self.0.notify_waiters()
        }
    }

    impl fmt::Debug for Notify {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod notify {
    use std::{
        cell::RefCell,
        fmt,
        future::Future,
        pin::Pin,
        rc::Rc,
        task::{Context, Poll, Waker},
    };

    #[derive(Clone)]
    pub struct Notify(Rc<RefCell<NotifyState>>);

    #[derive(Default)]
    struct NotifyState {
        generation: u64,
        waiters: Vec<Waker>,
    }

    impl Notify {
        pub fn new(_name: impl Into<String>) -> Self {
            Self(Rc::new(RefCell::new(NotifyState::default())))
        }

        pub fn notified(&self) -> Notified {
            Notified {
                state: self.0.clone(),
                observed: self.0.borrow().generation,
            }
        }

        pub fn notify_one(&self) {
            let mut state = self.0.borrow_mut();
            state.generation = state.generation.wrapping_add(1);
            if let Some(waker) = state.waiters.pop() {
                waker.wake();
            }
        }

        pub fn notify_waiters(&self) {
            let mut state = self.0.borrow_mut();
            state.generation = state.generation.wrapping_add(1);
            for waker in state.waiters.drain(..) {
                waker.wake();
            }
        }
    }

    pub struct Notified {
        state: Rc<RefCell<NotifyState>>,
        observed: u64,
    }

    impl Future for Notified {
        type Output = ();

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut state = self.state.borrow_mut();
            if state.generation != self.observed {
                return Poll::Ready(());
            }
            state.waiters.push(cx.waker().clone());
            Poll::Pending
        }
    }

    impl fmt::Debug for Notify {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Notify").finish_non_exhaustive()
        }
    }
}

pub use notify::*;

#[cfg(not(target_arch = "wasm32"))]
mod rwlock {
    #![expect(
        clippy::disallowed_types,
        reason = "vox-rt is the facade that wraps Tokio and parking_lot rwlocks"
    )]

    use std::fmt;

    pub struct RwLock<T>(tokio::sync::RwLock<T>);

    pub use tokio::sync::{
        RwLockReadGuard, RwLockWriteGuard, TryLockError as AsyncRwLockTryLockError,
    };

    impl<T> RwLock<T> {
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(tokio::sync::RwLock::new(value))
        }

        pub async fn read(&self) -> RwLockReadGuard<'_, T> {
            self.0.read().await
        }

        pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
            self.0.write().await
        }

        pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, AsyncRwLockTryLockError> {
            self.0.try_read()
        }

        pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, AsyncRwLockTryLockError> {
            self.0.try_write()
        }
    }

    impl<T: fmt::Debug> fmt::Debug for RwLock<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod sync_rwlock {
        use std::fmt;

        pub struct SyncRwLock<T>(parking_lot::RwLock<T>);

        pub use parking_lot::{
            RwLockReadGuard as SyncRwLockReadGuard, RwLockWriteGuard as SyncRwLockWriteGuard,
        };

        impl<T> SyncRwLock<T> {
            pub fn new(_name: &'static str, value: T) -> Self {
                Self(parking_lot::RwLock::new(value))
            }

            pub fn read(&self) -> SyncRwLockReadGuard<'_, T> {
                self.0.read()
            }

            pub fn write(&self) -> SyncRwLockWriteGuard<'_, T> {
                self.0.write()
            }

            pub fn try_read(&self) -> Option<SyncRwLockReadGuard<'_, T>> {
                self.0.try_read()
            }

            pub fn try_write(&self) -> Option<SyncRwLockWriteGuard<'_, T>> {
                self.0.try_write()
            }
        }

        impl<T: fmt::Debug> fmt::Debug for SyncRwLock<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    mod sync_rwlock {
        use std::fmt;

        pub struct SyncRwLock<T>(std::sync::RwLock<T>);

        pub type SyncRwLockReadGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;
        pub type SyncRwLockWriteGuard<'a, T> = std::sync::RwLockWriteGuard<'a, T>;

        impl<T> SyncRwLock<T> {
            pub fn new(_name: &'static str, value: T) -> Self {
                Self(std::sync::RwLock::new(value))
            }

            pub fn read(&self) -> SyncRwLockReadGuard<'_, T> {
                self.0.read().expect("vox runtime rwlock poisoned")
            }

            pub fn write(&self) -> SyncRwLockWriteGuard<'_, T> {
                self.0.write().expect("vox runtime rwlock poisoned")
            }

            pub fn try_read(&self) -> Option<SyncRwLockReadGuard<'_, T>> {
                match self.0.try_read() {
                    Ok(guard) => Some(guard),
                    Err(std::sync::TryLockError::WouldBlock) => None,
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        panic!("vox runtime rwlock poisoned")
                    }
                }
            }

            pub fn try_write(&self) -> Option<SyncRwLockWriteGuard<'_, T>> {
                match self.0.try_write() {
                    Ok(guard) => Some(guard),
                    Err(std::sync::TryLockError::WouldBlock) => None,
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        panic!("vox runtime rwlock poisoned")
                    }
                }
            }
        }

        impl<T: fmt::Debug> fmt::Debug for SyncRwLock<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    }

    pub use sync_rwlock::*;
}

#[cfg(target_arch = "wasm32")]
mod rwlock {
    use std::fmt;

    pub struct RwLock<T>(std::sync::RwLock<T>);

    pub type RwLockReadGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;
    pub type RwLockWriteGuard<'a, T> = std::sync::RwLockWriteGuard<'a, T>;

    #[derive(Debug)]
    pub struct AsyncRwLockTryLockError;

    impl<T> RwLock<T> {
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(std::sync::RwLock::new(value))
        }

        pub async fn read(&self) -> RwLockReadGuard<'_, T> {
            self.0.read().expect("vox runtime rwlock poisoned")
        }

        pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
            self.0.write().expect("vox runtime rwlock poisoned")
        }

        pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, AsyncRwLockTryLockError> {
            self.0.try_read().map_err(|_| AsyncRwLockTryLockError)
        }

        pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, AsyncRwLockTryLockError> {
            self.0.try_write().map_err(|_| AsyncRwLockTryLockError)
        }
    }

    impl<T: fmt::Debug> fmt::Debug for RwLock<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    pub struct SyncRwLock<T>(std::sync::RwLock<T>);

    pub type SyncRwLockReadGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;
    pub type SyncRwLockWriteGuard<'a, T> = std::sync::RwLockWriteGuard<'a, T>;

    impl<T> SyncRwLock<T> {
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(std::sync::RwLock::new(value))
        }

        pub fn read(&self) -> SyncRwLockReadGuard<'_, T> {
            self.0.read().expect("vox runtime rwlock poisoned")
        }

        pub fn write(&self) -> SyncRwLockWriteGuard<'_, T> {
            self.0.write().expect("vox runtime rwlock poisoned")
        }

        pub fn try_read(&self) -> Option<SyncRwLockReadGuard<'_, T>> {
            match self.0.try_read() {
                Ok(guard) => Some(guard),
                Err(std::sync::TryLockError::WouldBlock) => None,
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    panic!("vox runtime rwlock poisoned")
                }
            }
        }

        pub fn try_write(&self) -> Option<SyncRwLockWriteGuard<'_, T>> {
            match self.0.try_write() {
                Ok(guard) => Some(guard),
                Err(std::sync::TryLockError::WouldBlock) => None,
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    panic!("vox runtime rwlock poisoned")
                }
            }
        }
    }

    impl<T: fmt::Debug> fmt::Debug for SyncRwLock<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
}

pub use rwlock::*;

#[cfg(not(target_arch = "wasm32"))]
mod semaphore {
    use std::fmt;
    use std::sync::Arc;

    pub use tokio::sync::{AcquireError, OwnedSemaphorePermit, SemaphorePermit, TryAcquireError};

    #[derive(Clone)]
    pub struct Semaphore(Arc<tokio::sync::Semaphore>);

    impl Semaphore {
        pub fn new(_name: impl Into<String>, permits: usize) -> Self {
            Self(Arc::new(tokio::sync::Semaphore::new(permits)))
        }

        pub fn available_permits(&self) -> usize {
            self.0.available_permits()
        }

        pub fn close(&self) {
            self.0.close()
        }

        pub fn is_closed(&self) -> bool {
            self.0.is_closed()
        }

        pub fn add_permits(&self, n: usize) {
            self.0.add_permits(n)
        }

        pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
            self.0.acquire().await
        }

        pub async fn acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, AcquireError> {
            self.0.acquire_many(n).await
        }

        pub async fn acquire_owned(&self) -> Result<OwnedSemaphorePermit, AcquireError> {
            Arc::clone(&self.0).acquire_owned().await
        }

        pub async fn acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<OwnedSemaphorePermit, AcquireError> {
            Arc::clone(&self.0).acquire_many_owned(n).await
        }

        pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
            self.0.try_acquire()
        }

        pub fn try_acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, TryAcquireError> {
            self.0.try_acquire_many(n)
        }

        pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, TryAcquireError> {
            Arc::clone(&self.0).try_acquire_owned()
        }

        pub fn try_acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<OwnedSemaphorePermit, TryAcquireError> {
            Arc::clone(&self.0).try_acquire_many_owned(n)
        }
    }

    impl fmt::Debug for Semaphore {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod semaphore {
    use std::{
        cell::RefCell,
        fmt,
        future::Future,
        marker::PhantomData,
        pin::Pin,
        rc::Rc,
        task::{Context, Poll, Waker},
    };

    #[derive(Clone)]
    pub struct Semaphore(Rc<RefCell<SemaphoreState>>);

    struct SemaphoreState {
        permits: usize,
        closed: bool,
        waiters: Vec<Waker>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AcquireError;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TryAcquireError {
        Closed,
        NoPermits,
    }

    pub struct OwnedSemaphorePermit {
        semaphore: Option<Semaphore>,
        permits: usize,
    }

    pub struct SemaphorePermit<'a> {
        owned: OwnedSemaphorePermit,
        _marker: PhantomData<&'a Semaphore>,
    }

    impl fmt::Display for AcquireError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("semaphore closed")
        }
    }

    impl std::error::Error for AcquireError {}

    impl Semaphore {
        pub fn new(_name: impl Into<String>, permits: usize) -> Self {
            Self(Rc::new(RefCell::new(SemaphoreState {
                permits,
                closed: false,
                waiters: Vec::new(),
            })))
        }

        pub fn available_permits(&self) -> usize {
            self.0.borrow().permits
        }

        pub fn close(&self) {
            let mut state = self.0.borrow_mut();
            state.closed = true;
            for waker in state.waiters.drain(..) {
                waker.wake();
            }
        }

        pub fn is_closed(&self) -> bool {
            self.0.borrow().closed
        }

        pub fn add_permits(&self, n: usize) {
            let mut state = self.0.borrow_mut();
            state.permits = state.permits.saturating_add(n);
            for waker in state.waiters.drain(..) {
                waker.wake();
            }
        }

        pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
            let owned = self.acquire_owned().await?;
            Ok(SemaphorePermit {
                owned,
                _marker: PhantomData,
            })
        }

        pub async fn acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, AcquireError> {
            let owned = self.acquire_many_owned(n).await?;
            Ok(SemaphorePermit {
                owned,
                _marker: PhantomData,
            })
        }

        pub fn acquire_owned(&self) -> AcquireFuture {
            self.acquire_many_owned(1)
        }

        pub fn acquire_many_owned(&self, n: u32) -> AcquireFuture {
            AcquireFuture {
                semaphore: self.clone(),
                permits: n as usize,
            }
        }

        pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
            let owned = self.try_acquire_owned()?;
            Ok(SemaphorePermit {
                owned,
                _marker: PhantomData,
            })
        }

        pub fn try_acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, TryAcquireError> {
            let owned = self.try_acquire_many_owned(n)?;
            Ok(SemaphorePermit {
                owned,
                _marker: PhantomData,
            })
        }

        pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, TryAcquireError> {
            self.try_acquire_many_owned(1)
        }

        pub fn try_acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<OwnedSemaphorePermit, TryAcquireError> {
            let permits = n as usize;
            let mut state = self.0.borrow_mut();
            if state.closed {
                return Err(TryAcquireError::Closed);
            }
            if state.permits < permits {
                return Err(TryAcquireError::NoPermits);
            }
            state.permits -= permits;
            Ok(OwnedSemaphorePermit {
                semaphore: Some(self.clone()),
                permits,
            })
        }
    }

    pub struct AcquireFuture {
        semaphore: Semaphore,
        permits: usize,
    }

    impl Future for AcquireFuture {
        type Output = Result<OwnedSemaphorePermit, AcquireError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut state = self.semaphore.0.borrow_mut();
            if state.closed {
                return Poll::Ready(Err(AcquireError));
            }
            if state.permits >= self.permits {
                state.permits -= self.permits;
                return Poll::Ready(Ok(OwnedSemaphorePermit {
                    semaphore: Some(self.semaphore.clone()),
                    permits: self.permits,
                }));
            }
            state.waiters.push(cx.waker().clone());
            Poll::Pending
        }
    }

    impl Drop for OwnedSemaphorePermit {
        fn drop(&mut self) {
            if let Some(semaphore) = self.semaphore.take() {
                semaphore.add_permits(self.permits);
            }
        }
    }

    impl Drop for SemaphorePermit<'_> {
        fn drop(&mut self) {
            let _ = &self.owned;
        }
    }

    impl fmt::Debug for Semaphore {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let state = self.0.borrow();
            f.debug_struct("Semaphore")
                .field("permits", &state.permits)
                .field("closed", &state.closed)
                .finish()
        }
    }
}

pub use semaphore::*;

#[cfg(not(target_arch = "wasm32"))]
pub mod watch {
    pub use tokio::sync::watch::error::RecvError;
    pub use tokio::sync::watch::*;
}

#[cfg(target_arch = "wasm32")]
pub mod watch {
    use std::{
        cell::{Ref, RefCell},
        fmt,
        future::Future,
        pin::Pin,
        rc::Rc,
        task::{Context, Poll, Waker},
    };

    pub fn channel<T: Clone>(value: T) -> (Sender<T>, Receiver<T>) {
        let shared = Rc::new(RefCell::new(Shared {
            value,
            version: 0,
            sender_count: 1,
            waiters: Vec::new(),
        }));
        (
            Sender {
                shared: shared.clone(),
            },
            Receiver {
                shared,
                seen_version: 0,
            },
        )
    }

    struct Shared<T> {
        value: T,
        version: u64,
        sender_count: usize,
        waiters: Vec<Waker>,
    }

    pub struct Sender<T> {
        shared: Rc<RefCell<Shared<T>>>,
    }

    pub struct Receiver<T> {
        shared: Rc<RefCell<Shared<T>>>,
        seen_version: u64,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub struct SendError<T>(pub T);

    #[derive(Debug, PartialEq, Eq)]
    pub struct RecvError;

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            self.shared.borrow_mut().sender_count += 1;
            Self {
                shared: self.shared.clone(),
            }
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            let mut shared = self.shared.borrow_mut();
            shared.sender_count = shared.sender_count.saturating_sub(1);
            if shared.sender_count == 0 {
                for waker in shared.waiters.drain(..) {
                    waker.wake();
                }
            }
        }
    }

    impl<T> Sender<T> {
        pub fn send(&self, value: T) -> Result<(), SendError<T>> {
            let mut shared = self.shared.borrow_mut();
            shared.value = value;
            shared.version = shared.version.wrapping_add(1);
            for waker in shared.waiters.drain(..) {
                waker.wake();
            }
            Ok(())
        }
    }

    impl<T: fmt::Debug> fmt::Debug for Sender<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let shared = self.shared.borrow();
            f.debug_struct("Sender")
                .field("value", &shared.value)
                .field("version", &shared.version)
                .finish()
        }
    }

    impl<T> Clone for Receiver<T> {
        fn clone(&self) -> Self {
            Self {
                shared: self.shared.clone(),
                seen_version: self.seen_version,
            }
        }
    }

    impl<T> Receiver<T> {
        pub fn changed(&mut self) -> Changed<'_, T> {
            Changed { receiver: self }
        }

        pub fn borrow(&self) -> Ref<'_, T> {
            Ref::map(self.shared.borrow(), |shared| &shared.value)
        }
    }

    pub struct Changed<'a, T> {
        receiver: &'a mut Receiver<T>,
    }

    impl<T> Future for Changed<'_, T> {
        type Output = Result<(), RecvError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = unsafe { self.get_unchecked_mut() };
            let mut shared = this.receiver.shared.borrow_mut();
            if shared.version != this.receiver.seen_version {
                this.receiver.seen_version = shared.version;
                return Poll::Ready(Ok(()));
            }
            if shared.sender_count == 0 {
                return Poll::Ready(Err(RecvError));
            }
            shared.waiters.push(cx.waker().clone());
            Poll::Pending
        }
    }

    impl<T: fmt::Debug> fmt::Debug for Receiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Receiver")
                .field("value", &self.borrow())
                .finish()
        }
    }
}
