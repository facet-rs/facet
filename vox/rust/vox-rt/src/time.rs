use std::future::Future;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub mod error {
    pub use tokio::time::error::Elapsed;
}

#[cfg(target_arch = "wasm32")]
pub mod error {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Elapsed(());

    impl std::fmt::Display for Elapsed {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("deadline has elapsed")
        }
    }

    impl std::error::Error for Elapsed {}

    impl Elapsed {
        pub(crate) const fn new() -> Self {
            Self(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::std::Instant;

#[cfg(not(target_arch = "wasm32"))]
pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    tokio::time::sleep(duration)
}

#[cfg(target_arch = "wasm32")]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Interval(tokio::time::Interval);

#[cfg(target_arch = "wasm32")]
pub struct Interval {
    period: Duration,
    sleep: Option<Sleep>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissedTickBehavior {
    Burst,
    Delay,
    Skip,
}

impl Interval {
    pub async fn tick(&mut self) -> Instant {
        self.tick_inner().await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Interval {
    async fn tick_inner(&mut self) -> Instant {
        self.0.tick().await
    }

    pub fn set_missed_tick_behavior(&mut self, behavior: tokio::time::MissedTickBehavior) {
        self.0.set_missed_tick_behavior(behavior);
    }
}

#[cfg(target_arch = "wasm32")]
impl Interval {
    async fn tick_inner(&mut self) -> Instant {
        let sleep = self.sleep.get_or_insert_with(|| Sleep::new(self.period));
        sleep.await;
        self.sleep = Some(Sleep::new(self.period));
        Instant::now()
    }

    pub fn set_missed_tick_behavior(&mut self, _behavior: MissedTickBehavior) {}
}

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::time::MissedTickBehavior;

#[cfg(target_arch = "wasm32")]
pub struct Sleep {
    state: std::rc::Rc<std::cell::RefCell<SleepState>>,
}

#[cfg(target_arch = "wasm32")]
struct SleepState {
    duration: Duration,
    fired: bool,
    scheduled: bool,
    timeout_id: Option<i32>,
    callback: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
    waker: Option<std::task::Waker>,
}

#[cfg(target_arch = "wasm32")]
impl Sleep {
    fn new(duration: Duration) -> Self {
        Self {
            state: std::rc::Rc::new(std::cell::RefCell::new(SleepState {
                duration,
                fired: false,
                scheduled: false,
                timeout_id: None,
                callback: None,
                waker: None,
            })),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Future for Sleep {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use wasm_bindgen::JsCast;

        let mut state = self.state.borrow_mut();
        if state.fired {
            state.callback = None;
            return std::task::Poll::Ready(());
        }

        state.waker = Some(cx.waker().clone());
        if !state.scheduled {
            let shared = self.state.clone();
            let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                let mut state = shared.borrow_mut();
                state.fired = true;
                state.timeout_id = None;
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            }) as Box<dyn FnMut()>);
            let millis = i32::try_from(state.duration.as_millis()).unwrap_or(i32::MAX);
            let timeout_id = web_sys::window()
                .expect("wasm timer requires a browser Window")
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    millis,
                )
                .expect("schedule wasm timer");
            state.timeout_id = Some(timeout_id);
            state.callback = Some(callback);
            state.scheduled = true;
        }

        std::task::Poll::Pending
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Sleep {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        if let Some(timeout_id) = state.timeout_id.take()
            && let Some(window) = web_sys::window()
        {
            window.clear_timeout_with_handle(timeout_id);
        }
        state.callback = None;
    }
}

impl std::fmt::Debug for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interval").finish_non_exhaustive()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn interval(period: Duration) -> Interval {
    Interval(tokio::time::interval(period))
}

#[cfg(target_arch = "wasm32")]
pub fn interval(period: Duration) -> Interval {
    Interval {
        period,
        sleep: None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, error::Elapsed>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future).await
}

#[cfg(target_arch = "wasm32")]
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, error::Elapsed>
where
    F: Future<Output = T>,
{
    futures_util::pin_mut!(future);
    let sleep = sleep(duration);
    futures_util::pin_mut!(sleep);
    match futures_util::future::select(future, sleep).await {
        futures_util::future::Either::Left((value, _)) => Ok(value),
        futures_util::future::Either::Right(((), _)) => Err(error::Elapsed::new()),
    }
}
