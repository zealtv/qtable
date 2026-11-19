//! Gate 1, tests 1–5: the decision's edges, at synthetic instants.
use super::*;

const NS: fn(u64) -> Duration = Duration::from_nanos;
const US: fn(u64) -> Duration = Duration::from_micros;
const MS: fn(u64) -> Duration = Duration::from_millis;
/// 60 Hz.
const PERIOD: Duration = Duration::from_nanos(16_666_667);
const D: Duration = MAX_DEFERRAL;
const W: Duration = CLEARANCE;

/// A chain from planned instants alone, with the horizon far ahead unless given.
fn chain(planned: &[Instant], horizon: Instant, now: Instant) -> Chain {
    Chain::summarise(planned.iter().copied(), std::iter::empty(), horizon, now)
}

fn far(t: Instant) -> Instant {
    t + Duration::from_secs(1)
}

#[test]
fn the_cap_goes_at_t_plus_d_whatever_is_pending() {
    let t = Instant::now();
    // A closed chain ending by the cap: one hit just overdue, one due now.
    let at = |now: Instant| chain(&[now - US(100), now], far(t), now);
    assert_eq!(release(t, PERIOD, &at(t + D)), Release::Go);
    assert_eq!(release(t, PERIOD, &at(t + D - NS(1))), Release::Wait);
    // Overdue events never hold it at the cap.
    let now = t + D;
    let overdue = chain(&[t + US(10), t + US(20), now - US(5)], far(t), now);
    assert!(overdue.end.is_some());
    assert_eq!(release(t, PERIOD, &overdue), Release::Go);
    // An empty chain without visibility doesn't hold it at the cap either.
    assert_eq!(release(t, PERIOD, &chain(&[], t, now)), Release::Go);
}

#[test]
fn the_cap_is_never_more_than_half_a_period() {
    let t = Instant::now();
    let period = MS(6); // 166.7 Hz: half a period is 3 ms, under D.
    let now = t + MS(3);
    let pending = chain(&[now], far(t), now);
    assert_eq!(release(t, period, &pending), Release::Go);
    let now = t + MS(3) - NS(1);
    let pending = chain(&[now], far(t), now);
    assert_eq!(release(t, period, &pending), Release::Wait);
}

#[test]
fn an_empty_chain_goes_only_with_visibility() {
    let t = Instant::now();
    let now = t + US(200);
    // Nothing due within W, and the plan sees at least W ahead.
    let clear = chain(&[now + W + NS(1)], now + W, now);
    assert_eq!(clear.end, None);
    assert_eq!(release(t, PERIOD, &clear), Release::Go);
    // The plan sees less than W ahead: wait and replan.
    let short = chain(&[], now + W - NS(1), now);
    assert_eq!(release(t, PERIOD, &short), Release::Replan);
    // A horizon in the past.
    let past = chain(&[], now - MS(3), now);
    assert!(!past.visible());
    assert_eq!(release(t, PERIOD, &past), Release::Replan);
    // A non-empty chain never reaches cases 2 or 3, with or without visibility.
    let known = chain(&[now + US(10)], now - MS(3), now);
    assert_eq!(release(t, PERIOD, &known), Release::Wait);
}

#[test]
fn chaining_links_a_gap_of_exactly_w_and_breaks_beyond_it() {
    let now = Instant::now();
    let first = now + US(400);
    let linked = chain(&[first, first + W, first + W + W], far(now), now);
    assert_eq!(linked.end, Some(first + W + W));
    let broken = chain(&[first, first + W + NS(1)], far(now), now);
    assert_eq!(broken.end, Some(first));
    // The first event links to now the same way.
    assert_eq!(chain(&[now + W], far(now), now).end, Some(now + W));
    assert_eq!(chain(&[now + W + NS(1)], far(now), now).end, None);
}

#[test]
fn manual_hits_and_planned_events_chain_together_in_due_order() {
    let now = Instant::now();
    let planned = [now + US(100), now + US(2_500)];
    let manual = [now + US(900), now + US(1_800)];
    let merged = Chain::summarise(
        planned.iter().copied(),
        manual.iter().copied(),
        far(now),
        now,
    );
    // 0.1 → 0.9 (manual) → 1.8 (manual) → 2.5 ms: every gap is under W.
    assert_eq!(merged.end, Some(now + US(2_500)));
    let manual_only = Chain::summarise(std::iter::empty(), manual.iter().copied(), far(now), now);
    assert_eq!(manual_only.end, Some(now + US(1_800)));
}

#[test]
fn a_chain_ending_at_the_cap_waits_and_one_past_it_goes() {
    let t = Instant::now();
    let now = t + US(500);
    let steps = |end: Instant| {
        let mut at = vec![];
        let mut next = now + US(100);
        while next < end {
            at.push(next);
            next += US(500);
        }
        at.push(end);
        at
    };
    assert_eq!(
        release(t, PERIOD, &chain(&steps(t + D), far(t), now)),
        Release::Wait
    );
    assert_eq!(
        release(t, PERIOD, &chain(&steps(t + D + NS(1)), far(t), now)),
        Release::Go
    );
}

#[test]
fn a_chain_reaching_the_horizon_is_open_and_waited_for_up_to_the_cap() {
    let t = Instant::now();
    let now = t + US(100);
    let end = now + US(600);
    // end + W > E: something beyond E could still join.
    let open = chain(&[now + US(300), end], end + W - NS(1), now);
    assert!(open.open());
    assert_eq!(release(t, PERIOD, &open), Release::Wait);
    let closed = chain(&[now + US(300), end], end + W, now);
    assert!(!closed.open());
    assert_eq!(release(t, PERIOD, &closed), Release::Wait);
    // Insufficient visibility makes even an empty chain open.
    assert!(chain(&[], now, now).open());
    // At the cap the open chain goes too.
    let late = t + D;
    let open = chain(&[late + US(300)], late + US(400), late);
    assert!(open.open());
    assert_eq!(release(t, PERIOD, &open), Release::Go);
}
