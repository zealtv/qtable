//! complete-clusters-before-frames, gate 1 test 12a: passes that jump across the horizon E.
use super::*;
use crate::scheduler::FrameProfile;

const US: fn(u64) -> Duration = Duration::from_micros;

/// A frame due at the 20 ms tick (50 Hz), with diagnostics.
fn due_at_20() -> (Project, Rig, mpsc::Receiver<SchedulerEvent>, EventSink) {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut rig = Rig::new(&p);
    rig.stages.diagnostics = true;
    set_volume(&mut p, 0.4);
    rig.update(&p, 5);
    let (tx, rx) = mpsc::sync_channel(100);
    (p, rig, rx, EventSink::new(tx))
}

fn pass(rig: &mut Rig, p: &Project, sink: &mut EventSink, chain: cluster::Chain) {
    rig.stages
        .dispatch(p, &mut rig.sender, &rig.activity, sink, &chain);
}

fn frames(rx: &mpsc::Receiver<SchedulerEvent>) -> Vec<FrameProfile> {
    rx.try_iter()
        .filter_map(|event| match event {
            SchedulerEvent::FrameSent { profile, .. } => Some(profile),
            _ => None,
        })
        .collect()
}

#[test]
fn passes_across_the_horizon_wait_and_replan_and_a_pass_past_the_cap_releases_at_once() {
    let (p, mut rig, rx, mut sink) = due_at_20();
    let tick = ms(rig.anchor, 20);
    let horizon = tick + US(900);
    // E − 100 µs: nothing pending is due within W, but the plan sees only 100 µs ahead.
    let now = horizon - US(100);
    let chain = cluster::Chain::summarise(std::iter::empty(), std::iter::empty(), horizon, now);
    pass(&mut rig, &p, &mut sink, chain);
    assert!(
        frames(&rx).is_empty(),
        "held back on an empty-looking chain"
    );
    assert!(rig.stages.take_replan());
    assert!(!rig.stages.take_replan(), "taken once");
    // E + 300 µs, before any replan: the pending event due at E is now overdue.
    let now = horizon + US(300);
    let chain = cluster::Chain::summarise([horizon].into_iter(), std::iter::empty(), horizon, now);
    pass(&mut rig, &p, &mut sink, chain);
    assert!(frames(&rx).is_empty(), "waits for the overdue event");
    assert!(!rig.stages.take_replan());
    // The worker stalls; its next pass comes at T + D + 200 µs, with the event still pending.
    let now = tick + Duration::from_millis(4) + US(200);
    let chain = cluster::Chain::summarise([horizon].into_iter(), std::iter::empty(), horizon, now);
    pass(&mut rig, &p, &mut sink, chain);
    let profiles = frames(&rx);
    assert_eq!(profiles.len(), 1, "released at once");
    assert_eq!(profiles[0].released, Duration::from_millis(4) + US(200));
    assert_eq!(profiles[0].withheld_last, Some(US(1_200)), "below T + D");
}

#[test]
fn a_frame_never_held_back_reports_no_withheld_pass() {
    let (p, mut rig, rx, mut sink) = due_at_20();
    let now = ms(rig.anchor, 20) + US(30);
    pass(&mut rig, &p, &mut sink, clear(now, None));
    let profiles = frames(&rx);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].released, US(30));
    assert_eq!(profiles[0].withheld_last, None);
}
