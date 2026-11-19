//! Send timing policy for the real-clock integration tests.
//!
//! As of 2026-10-11, timed-test-flake records a >1 ms send outlier during a
//! concurrent check. Mach scheduling is soft realtime: OS preemption can affect
//! sends as well as receiver arrivals. Always report samples in the caller;
//! keep the original strict limits for quiet-machine measurements, without
//! retrying or discarding a failed sample. Default bounds catch sustained
//! lateness and gross stalls under load; functional assertions remain active.
//!
//! Run with QTABLE_ASSERT_SEND=1 cargo test -p qtable-engine -- --nocapture.
pub fn assert_send_timing(mean_ms: f64, max_ms: f64) {
    assert!(mean_ms.is_finite() && max_ms.is_finite());
    // Mean is the main signal for slow planning or a broken wait loop; max
    // tolerates occasional preemption but still catches gross stalls. As measured when
    // these limits were set, a loaded run's mean sat near 0.09 ms and a plain timed sleep's near
    // 2.3 ms, so the 2 ms mean limit leaves headroom over the first and still fails the
    // second; the 25 ms max sits above the stressed maxima seen.
    assert!(
        mean_ms < 2.0,
        "mean send lateness {mean_ms:.3} ms (limit 2 ms)"
    );
    assert!(
        max_ms < 25.0,
        "max send lateness {max_ms:.3} ms (limit 25 ms)"
    );
    if std::env::var_os("QTABLE_ASSERT_SEND").is_some_and(|value| value == "1") {
        assert!(max_ms < 1.0, "max send lateness {max_ms:.3} ms");
        assert!(mean_ms < 0.2, "mean send lateness {mean_ms:.3} ms");
    } else {
        eprintln!("load-tolerant send timing asserted; set QTABLE_ASSERT_SEND=1 for strict limits");
    }
}
