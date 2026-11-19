//! macOS scheduling setup; no work or policy calls in the send loop.

/// Request a 1 ms nominal period with 0.5 ms computation within 1 ms.
/// Mach uses CPU-dependent absolute ticks, not nanoseconds. This is soft
/// realtime: the kernel can preempt or demote a thread, including a busy spinner.
#[allow(deprecated)] // libc 0.2.190 exposes mach_timebase_info; avoid another FFI crate.
fn set_time_constraint() -> Result<(), libc::kern_return_t> {
    let mut timebase = libc::mach_timebase_info { numer: 0, denom: 0 };
    // SAFETY: writable, correctly sized timebase; initialized by the system call.
    let result = unsafe { libc::mach_timebase_info(&mut timebase) };
    if result != libc::KERN_SUCCESS {
        return Err(result);
    }
    if timebase.numer == 0 || timebase.denom == 0 {
        return Err(libc::KERN_INVALID_ARGUMENT);
    }
    let ticks = |ns: u64| {
        u32::try_from(ns * u64::from(timebase.denom) / u64::from(timebase.numer))
            .ok()
            .filter(|value| *value > 0)
            .ok_or(libc::KERN_INVALID_ARGUMENT)
    };
    let mut policy = libc::thread_time_constraint_policy {
        period: ticks(1_000_000)?,
        computation: ticks(500_000)?,
        constraint: ticks(1_000_000)?,
        preemptible: 1,
    };
    // SAFETY: borrowed port for this live pthread (no send-right allocation);
    // policy layout and integer count are supplied by libc for this flavor.
    let result = unsafe {
        libc::thread_policy_set(
            libc::pthread_mach_thread_np(libc::pthread_self()),
            libc::THREAD_TIME_CONSTRAINT_POLICY as libc::thread_policy_flavor_t,
            (&mut policy as *mut libc::thread_time_constraint_policy).cast(),
            libc::THREAD_TIME_CONSTRAINT_POLICY_COUNT,
        )
    };
    if result == libc::KERN_SUCCESS {
        Ok(())
    } else {
        Err(result)
    }
}

pub(super) fn configure() {
    // SAFETY: affects only this thread; class and relative priority are valid.
    let qos = unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0)
    };
    if qos != 0 {
        eprintln!("qtable scheduler: USER_INTERACTIVE QoS failed: {qos}");
    }
    // Explicit troubleshooting/A/B switch. Checked once, before playback.
    if std::env::var_os("QTABLE_DISABLE_REALTIME").is_some() {
        if std::env::var_os("QTABLE_SCHEDULER_POLICY_DIAGNOSTICS").is_some() {
            eprintln!("qtable scheduler: realtime disabled; QoS result={qos}");
        }
        return;
    }
    match set_time_constraint() {
        Ok(()) => {
            if std::env::var_os("QTABLE_SCHEDULER_POLICY_DIAGNOSTICS").is_some() {
                eprintln!("qtable scheduler: realtime policy accepted (1/0.5/1 ms)");
            }
        }
        // A scheduling hint failing must not abort otherwise valid playback.
        Err(code) => eprintln!(
            "qtable scheduler: realtime policy failed (Mach code {code}); keeping QoS fallback"
        ),
    }
}
