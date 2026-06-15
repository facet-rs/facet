// r[impl rpc.channel.delivery.reliable] r[impl rpc.flow-control.credit]

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

pub mod oneshot {
    pub use tokio::sync::oneshot::{Receiver, Sender, error};

    pub fn channel<T>(_name: impl Into<String>) -> (Sender<T>, Receiver<T>) {
        tokio::sync::oneshot::channel()
    }
}

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

pub use mutex::*;

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

pub use notify::*;

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

pub use rwlock::*;

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

pub use semaphore::*;
