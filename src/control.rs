//! Cooperative cancellation and the conversion-wide deadline.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::config::Config;
use crate::error::{CrateError, Result};

/// A flag the host sets to make a running conversion stop.
///
/// Clone it freely: all clones share one flag. Attach one to a conversion with
/// [`Config::with_cancel_token`](crate::Config::with_cancel_token) and call [`CancelToken::cancel`]
/// from any thread; the conversion returns [`CrateError::Cancelled`](crate::CrateError) at its next
/// checkpoint (between Markdown events, between renders, every few milliseconds while an external
/// tool runs, and before the archive is built), killing a running external tool's process tree first.
///
/// Cancellation is **cooperative**. A synchronous callback (a [`FenceRenderer`](crate::FenceRenderer),
/// a highlighter) is not interrupted; it can poll [`RenderContext::is_cancelled`](crate::RenderContext).
/// A host that needs a hard deadline must still terminate the worker from outside.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Shared>);

type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    /// Callbacks to run once, when the token is cancelled, keyed by registration id.
    wakers: Mutex<Vec<(u64, Waker)>>,
    next_id: AtomicU64,
}

impl std::fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask every conversion holding this token to stop, and run the callbacks registered with
    /// [`CancelToken::register`] (on this thread, once).
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let wakers: Vec<_> = {
            let Ok(mut list) = self.0.wakers.lock() else { return };
            std::mem::take(&mut *list).into_iter().map(|(_, w)| w).collect()
        };
        for wake in wakers {
            wake();
        }
    }

    /// True once [`CancelToken::cancel`] has been called on any clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    /// Run `on_cancel` when the token is cancelled (at once if it already is). The callback is how a
    /// cooperative callback bridges this token into another library's own cancellation handle,
    /// so a long render can be interrupted rather than only noticed afterwards. Keep it short and
    /// non-blocking: it runs on the thread that calls [`CancelToken::cancel`]. Dropping the returned
    /// [`Registration`] removes the callback.
    pub fn register(&self, on_cancel: impl Fn() + Send + Sync + 'static) -> Registration {
        let callback: Waker = Arc::new(on_cancel);
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut list) = self.0.wakers.lock() {
            if !self.is_cancelled() {
                list.push((id, callback));
                return Registration {
                    shared: Some(Arc::clone(&self.0)),
                    id,
                };
            }
        }
        callback();
        Registration { shared: None, id }
    }
}

/// Keeps a [`CancelToken::register`] callback alive; dropping it unregisters the callback.
#[must_use = "the callback is removed when the registration is dropped"]
pub struct Registration {
    shared: Option<Arc<Shared>>,
    id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(shared) = &self.shared {
            if let Ok(mut list) = shared.wakers.lock() {
                list.retain(|(id, _)| *id != self.id);
            }
        }
    }
}

/// What one conversion checks at its checkpoints: the host's token and the wall-clock deadline.
#[derive(Debug, Clone)]
pub(crate) struct Ctl {
    cancel: Option<CancelToken>,
    deadline: Option<Instant>,
}

impl Ctl {
    pub(crate) fn new(cfg: &Config) -> Self {
        Ctl {
            cancel: cfg.cancel.clone(),
            deadline: cfg
                .limits
                .max_conversion_time
                .and_then(|d| Instant::now().checked_add(d)),
        }
    }

    #[cfg(test)]
    pub(crate) fn none() -> Self {
        Ctl {
            cancel: None,
            deadline: None,
        }
    }

    pub(crate) fn cancel_token(&self) -> Option<&CancelToken> {
        self.cancel.as_ref()
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Fail if the host cancelled the conversion or its time limit has passed.
    pub(crate) fn check(&self) -> Result<()> {
        if self.cancel.as_ref().is_some_and(CancelToken::is_cancelled) {
            return Err(CrateError::Cancelled("the conversion was cancelled".into()));
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(timed_out());
        }
        Ok(())
    }
}

pub(crate) fn timed_out() -> CrateError {
    CrateError::TimedOut("the conversion exceeded Limits::max_conversion_time".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn registered_callbacks_run_once_on_cancel_and_not_after_being_dropped() {
        let token = CancelToken::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let kept = token.register(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let dropped_counter = Arc::clone(&hits);
        drop(token.register(move || {
            dropped_counter.fetch_add(100, Ordering::SeqCst);
        }));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        token.cancel();
        token.cancel();
        assert_eq!(hits.load(Ordering::SeqCst), 1, "once, and the dropped one never");
        // Registering after the fact runs the callback at once.
        let late = Arc::clone(&hits);
        let _r = token.register(move || {
            late.fetch_add(10, Ordering::SeqCst);
        });
        assert_eq!(hits.load(Ordering::SeqCst), 11);
        drop(kept);
        assert!(token.clone().is_cancelled(), "clones share the flag");
    }
}
