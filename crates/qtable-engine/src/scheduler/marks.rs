//! Diagnostics only: named instants on the scheduler thread, so a measurement can see where a
//! command's or a boundary's time goes before anything is sent. Off, and nearly free, unless the
//! scheduler runs with diagnostics; the worker drains them into `SchedulerEvent::Marks`.
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

thread_local! {
    static MARKS: RefCell<Option<Vec<(&'static str, Instant)>>> = const { RefCell::new(None) };
    /// The last worker pass: when it began and the thread's CPU time then.
    static PASS: Cell<Option<(Instant, Option<Duration>)>> = const { Cell::new(None) };
}

fn enabled() -> bool {
    MARKS.with(|marks| marks.borrow().is_some())
}

/// Start recording on this thread.
pub(super) fn enable() {
    MARKS.with(|marks| *marks.borrow_mut() = Some(Vec::new()));
}

/// Record `label` now, if recording.
pub(super) fn mark(label: &'static str) {
    MARKS.with(|marks| {
        if let Some(list) = marks.borrow_mut().as_mut() {
            list.push((label, Instant::now()));
        }
    });
}

/// Record `label` at an instant taken earlier, if recording.
pub(super) fn mark_at(label: &'static str, at: Instant) {
    MARKS.with(|marks| {
        if let Some(list) = marks.borrow_mut().as_mut() {
            list.push((label, at));
        }
    });
}

/// Run `f`, recording `begin` and `end` around it if recording and it took at least `over`.
pub(super) fn span<R>(
    begin: &'static str,
    end: &'static str,
    over: Duration,
    f: impl FnOnce() -> R,
) -> R {
    if !enabled() {
        return f();
    }
    let start = Instant::now();
    let result = f();
    let finish = Instant::now();
    if finish - start >= over {
        mark_at(begin, start);
        mark_at(end, finish);
    }
    result
}

/// Note the start of a worker pass. If the last pass began at least `over` earlier, record
/// `gap.begin` then and `gap.end` now, plus `gap.cpu` at `gap.begin` + the thread's CPU time in
/// between, so that work on the thread can be told from time spent off-CPU.
pub(super) fn pass(over: Duration) {
    if !enabled() {
        return;
    }
    let now = (Instant::now(), thread_cpu());
    if let Some((last, last_cpu)) = PASS.replace(Some(now))
        && now.0 - last >= over
    {
        mark_at("gap.begin", last);
        if let (Some(cpu), Some(last_cpu)) = (now.1, last_cpu) {
            mark_at("gap.cpu", last + cpu.saturating_sub(last_cpu));
        }
        mark_at("gap.end", now.0);
    }
}

#[cfg(target_os = "macos")]
fn thread_cpu() -> Option<Duration> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid out-pointer for this thread's CPU clock.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) } == 0;
    ok.then(|| Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
}

#[cfg(not(target_os = "macos"))]
fn thread_cpu() -> Option<Duration> {
    None
}

/// How many marks are waiting, so a caller can drop the ones that turned out not to matter.
pub(super) fn len() -> usize {
    MARKS.with(|marks| marks.borrow().as_ref().map_or(0, Vec::len))
}

pub(super) fn truncate(len: usize) {
    MARKS.with(|marks| {
        if let Some(list) = marks.borrow_mut().as_mut() {
            list.truncate(len);
        }
    });
}

/// The marks recorded since the last call, oldest first.
pub(super) fn take() -> Vec<(&'static str, Instant)> {
    MARKS.with(|marks| {
        marks
            .borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_recorded_only_once_enabled() {
        std::thread::spawn(|| {
            mark("ignored");
            assert_eq!(len(), 0);
            enable();
            mark("a");
            let before = len();
            mark("b");
            truncate(before);
            mark("c");
            let taken: Vec<_> = take().into_iter().map(|(label, _)| label).collect();
            assert_eq!(taken, ["a", "c"]);
            assert!(take().is_empty());
            span("quick.begin", "quick.end", Duration::from_secs(1), || ());
            span("slow.begin", "slow.end", Duration::ZERO, || ());
            pass(Duration::ZERO);
            pass(Duration::ZERO);
            let taken: Vec<_> = take().into_iter().map(|(label, _)| label).collect();
            assert_eq!(&taken[..2], ["slow.begin", "slow.end"]);
            assert_eq!(taken[2], "gap.begin");
            assert_eq!(taken.last(), Some(&"gap.end"));
        })
        .join()
        .unwrap();
    }
}
