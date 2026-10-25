//! Cooperative cancellation and the conversion-wide deadline.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::config::Config;
use crate::error::{CrateError, Result};

/// A flag the host sets to make a running conversion stop.
///
/// Clone it freely: all clones share one flag. Attach one to a conversion with
/// [`Config::with_cancel_token`](crate::Config::with_cancel_token) and call [`CancelToken::cancel`]
/// from any thread; the conversion returns [`CrateError::Cancelled`](crate::CrateError) at its next
/// checkpoint (between Markdown events, between renders, every few milliseconds while an external
/// tool runs, and between archive entries), killing a running external tool's process tree first.
///
/// Cancellation is **cooperative**. A synchronous callback (a [`FenceRenderer`](crate::FenceRenderer),
/// a highlighter) is not interrupted; it can poll [`RenderContext::is_cancelled`](crate::RenderContext).
/// A host that needs a hard deadline must still terminate the worker from outside.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask every conversion holding this token to stop.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// True once [`CancelToken::cancel`] has been called on any clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
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
