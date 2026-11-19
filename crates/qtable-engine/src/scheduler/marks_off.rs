//! The marks' stand-in when the `dev-hooks` feature is off: the same calls, doing nothing.
use std::time::{Duration, Instant};

pub(super) fn mark(_label: &'static str) {}

pub(super) fn mark_at(_label: &'static str, _at: Instant) {}

pub(super) fn span<R>(
    _begin: &'static str,
    _end: &'static str,
    _over: Duration,
    f: impl FnOnce() -> R,
) -> R {
    f()
}

pub(super) fn pass(_over: Duration) {}

pub(super) fn len() -> usize {
    0
}

pub(super) fn truncate(_len: usize) {}
