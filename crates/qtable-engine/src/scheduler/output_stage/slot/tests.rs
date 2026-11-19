use super::*;
use crate::scheduler::parameter_controls::SendValue;

/// What an unbudgeted frame does: every pick goes out.
#[derive(Debug, Default, PartialEq)]
struct Outcome {
    send: Option<f32>,
    merged: u64,
}

impl Slot {
    fn frame(&mut self, at: Instant, period: Duration, runs: &[Run]) -> Outcome {
        let outcome = self.prepare(at, period, runs, false);
        let send = outcome.pick.map(|pick| match pick.value.0[0] {
            SendValue::Float(value) => value,
            _ => panic!("expected float"),
        });
        if let Some(value) = send {
            self.sent(value);
        }
        Outcome {
            send,
            merged: outcome.merged,
        }
    }
}

const FRAME: Duration = Duration::from_millis(20);

fn ms(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

fn settings(mode: SendMode, threshold: f64, delay_ms: f64) -> Settings {
    Settings {
        mode,
        thresholds: Arc::from([threshold]),
        delay_ms,
    }
}

fn continuous() -> Settings {
    settings(SendMode::Continuous, 0.0, 0.0)
}

fn slot(start: Instant, settings: Settings) -> Slot {
    Slot::quiet("/volume".into(), 1.0, settings, start)
}

/// What each frame sends, at these milliseconds after `start`.
fn sends(slot: &mut Slot, start: Instant, frames: &[u64], runs: &[Run]) -> Vec<Option<f32>> {
    frames
        .iter()
        .map(|&at| slot.frame(ms(start, at), FRAME, runs).send)
        .collect()
}

fn wants(slot: &Slot, runs: &[Run]) -> Vec<Want> {
    let mut wants = vec![];
    slot.wants(FRAME, runs, |want| wants.push(want));
    wants
}

#[test]
fn a_quiet_slot_owes_nothing_and_a_new_one_sends_once() {
    let start = Instant::now();
    let mut quiet = slot(start, continuous());
    assert!(wants(&quiet, &[]).is_empty());
    assert_eq!(sends(&mut quiet, start, &[0, 20], &[]), [None, None]);
    let mut new = Slot::new("/volume".into(), 1.0, continuous(), start);
    assert_eq!(
        wants(&new, &[]),
        [Want::Frame {
            from: start,
            until: None
        }]
    );
    assert_eq!(sends(&mut new, start, &[0, 20], &[]), [Some(1.0), None]);
    assert!(wants(&new, &[]).is_empty());
}

#[test]
fn the_latest_value_wins_and_overwritten_values_count_as_merged() {
    let start = Instant::now();
    let mut slot = slot(start, continuous());
    for (index, value) in [0.1, 0.2, 0.3, 0.4].into_iter().enumerate() {
        slot.edit(value, ms(start, 1 + index as u64));
    }
    let outcome = slot.frame(ms(start, 20), FRAME, &[]);
    assert_eq!(
        outcome,
        Outcome {
            send: Some(0.4),
            merged: 3
        }
    );
    assert_eq!(slot.frame(ms(start, 40), FRAME, &[]), Outcome::default());
}

#[test]
fn compaction_keeps_the_newest_eligible_typed_value_and_the_delay_window() {
    let start = Instant::now();
    let mut slot = Slot::quiet(
        "/value".into(),
        SendValue::Int(16_777_217),
        settings(SendMode::Continuous, 0.0, 30.0),
        start,
    );
    for edit in 1..=100 {
        slot.edit(SendValue::Int(16_777_217 + edit), ms(start, edit as u64));
    }
    // Edit 70 is eligible at 100 ms; edits 71 through 100 are still delayed.
    assert_eq!(slot.history.len(), 31);
    let outcome = slot.prepare(ms(start, 100), FRAME, &[], false);
    assert_eq!(
        outcome.pick.unwrap().value,
        SendValues::from(SendValue::Int(16_777_287))
    );
    assert_eq!(outcome.merged, 69);
    slot.sent(SendValue::Int(16_777_287));
    let outcome = slot.prepare(ms(start, 130), FRAME, &[], false);
    assert_eq!(
        outcome.pick.unwrap().value,
        SendValues::from(SendValue::Int(16_777_317))
    );
    assert_eq!(outcome.merged, 29);
    slot.sent(SendValue::Int(16_777_317));
    assert_eq!(slot.prepare(ms(start, 150), FRAME, &[], false).merged, 0);
}

#[test]
fn compaction_preserves_an_unsent_starts_force_and_off_grid_send() {
    let start = Instant::now();
    let mut slot = slot(start, settings(SendMode::Continuous, 0.5, 0.0));
    slot.edit(0.5, ms(start, 1));
    assert_eq!(sends(&mut slot, start, &[20], &[]), [Some(0.5)]);
    slot.start(ms(start, 25));
    slot.edit(0.6, ms(start, 26));
    slot.edit(0.5, ms(start, 27));
    assert_eq!(slot.history.len(), 1);
    assert!(slot.starts_by(ms(start, 27)));
    assert_eq!(wants(&slot, &[]), [Want::At(ms(start, 27))]);
    let outcome = slot.prepare(ms(start, 27), FRAME, &[], false);
    assert!(outcome.pick.unwrap().forced);
    assert_eq!(outcome.merged, 2);
    slot.sent(0.5);
    assert_eq!(slot.frame(ms(start, 40), FRAME, &[]), Outcome::default());
}

#[test]
fn equal_bits_are_not_resent() {
    let start = Instant::now();
    let mut slot = slot(start, continuous());
    slot.edit(0.5, ms(start, 1));
    assert_eq!(sends(&mut slot, start, &[20], &[]), [Some(0.5)]);
    // Away and back within one frame: nothing new reaches the receiver.
    slot.edit(0.6, ms(start, 21));
    slot.edit(0.5, ms(start, 22));
    assert_eq!(
        slot.frame(ms(start, 40), FRAME, &[]),
        Outcome {
            send: None,
            merged: 1
        }
    );
    assert!(wants(&slot, &[]).is_empty());
    // Editing to the current value adds nothing to send.
    slot.edit(0.5, ms(start, 41));
    assert!(wants(&slot, &[]).is_empty());
}

#[test]
fn threshold_holds_small_changes_until_they_settle_for_a_frame() {
    let start = Instant::now();
    let mut slot = slot(start, settings(SendMode::Continuous, 0.1, 0.0));
    slot.edit(0.5, ms(start, 1));
    assert_eq!(sends(&mut slot, start, &[20], &[]), [Some(0.5)]);
    slot.edit(0.55, ms(start, 25));
    assert_eq!(
        wants(&slot, &[]),
        [Want::Frame {
            from: ms(start, 45),
            until: None
        }]
    );
    assert_eq!(sends(&mut slot, start, &[40], &[]), [None]);
    // A newer small edit restarts the wait.
    slot.edit(0.56, ms(start, 50));
    assert_eq!(sends(&mut slot, start, &[60, 80], &[]), [None, Some(0.56)]);
    // A change of at least the threshold goes at once.
    slot.edit(0.7, ms(start, 81));
    assert_eq!(sends(&mut slot, start, &[100], &[]), [Some(0.7)]);
}

#[test]
fn delay_makes_edits_eligible_late_and_negative_delay_at_once() {
    let start = Instant::now();
    let mut slot = slot(start, settings(SendMode::Continuous, 0.0, 30.0));
    slot.edit(0.5, ms(start, 5));
    assert_eq!(
        wants(&slot, &[]),
        [Want::Frame {
            from: ms(start, 35),
            until: None
        }]
    );
    assert_eq!(sends(&mut slot, start, &[20, 40], &[]), [None, Some(0.5)]);
    slot.settings.delay_ms = -30.0;
    slot.edit(0.6, ms(start, 45));
    assert_eq!(sends(&mut slot, start, &[60], &[]), [Some(0.6)]);
}

#[test]
fn changing_mode_threshold_or_delay_alone_sends_nothing() {
    let start = Instant::now();
    let mut slot = slot(start, continuous());
    slot.settings = settings(SendMode::OnRelease, 0.2, 10.0);
    slot.settings = settings(SendMode::EveryFrame, 0.0, -10.0);
    assert!(wants(&slot, &[]).is_empty());
    assert_eq!(sends(&mut slot, start, &[20, 40], &[]), [None, None]);
}

#[test]
fn start_resends_an_unchanged_value_at_its_eligible_instant() {
    let start = Instant::now();
    let mut slot = slot(start, settings(SendMode::Continuous, 0.5, 10.0));
    slot.edit(0.5, start);
    assert_eq!(sends(&mut slot, start, &[20], &[]), [Some(0.5)]);
    slot.start(ms(start, 25));
    assert!(!slot.starts_by(ms(start, 34)));
    assert!(slot.starts_by(ms(start, 35)));
    assert_eq!(wants(&slot, &[]), [Want::At(ms(start, 35))]);
    assert_eq!(sends(&mut slot, start, &[35], &[]), [Some(0.5)]);
    // An edit that overtakes the start value keeps its claim to be sent.
    slot.start(ms(start, 50));
    slot.edit(0.6, ms(start, 51));
    slot.edit(0.5, ms(start, 52));
    assert_eq!(
        slot.frame(ms(start, 70), FRAME, &[]),
        Outcome {
            send: Some(0.5),
            merged: 2
        }
    );
}

#[test]
fn every_frame_resends_each_frame_of_a_run_and_otherwise_acts_as_continuous() {
    let start = Instant::now();
    let mut slot = slot(start, settings(SendMode::EveryFrame, 0.5, 0.0));
    let run = [Run {
        start: ms(start, 30),
        stop: None,
    }];
    // Stopped: one send per change; the threshold doesn't apply in Every Frame.
    slot.edit(0.9, ms(start, 1));
    assert_eq!(sends(&mut slot, start, &[20], &[]), [Some(0.9)]);
    assert_eq!(
        wants(&slot, &run),
        [Want::Frame {
            from: ms(start, 30),
            until: None
        }]
    );
    assert_eq!(
        sends(&mut slot, start, &[20, 40, 60], &run),
        [None, Some(0.9), Some(0.9)]
    );
}

#[test]
fn every_frame_keeps_the_delayed_tail_before_stop_and_its_snapshots() {
    let start = Instant::now();
    let mut slot = slot(start, settings(SendMode::EveryFrame, 0.0, 200.0));
    slot.start(start);
    let run = [Run {
        start,
        stop: Some(ms(start, 55)),
    }];
    slot.edit(0.4, ms(start, 30));
    slot.edit(0.7, ms(start, 60));
    assert_eq!(
        wants(&slot, &run),
        [
            Want::At(ms(start, 200)),
            Want::Frame {
                from: ms(start, 230),
                until: None
            },
            Want::Frame {
                from: ms(start, 260),
                until: None
            },
            Want::Frame {
                from: ms(start, 200),
                until: Some(ms(start, 255))
            },
        ]
    );
    // Each tail frame sends the value edited by its musical time, then the tail ends and the
    // edit made after Stop goes once.
    assert_eq!(
        sends(&mut slot, start, &[200, 220, 240, 260, 280], &run),
        [Some(1.0), Some(1.0), Some(0.4), Some(0.7), None]
    );
}

#[test]
fn every_frame_skips_frames_before_the_slot_had_a_value() {
    let start = Instant::now();
    let run = [Run { start, stop: None }];
    let mut slot = Slot::new(
        "/volume".into(),
        1.0,
        settings(SendMode::EveryFrame, 0.0, 45.0),
        ms(start, 70),
    );
    assert_eq!(
        sends(&mut slot, start, &[45, 65, 85, 105], &run),
        [None, None, Some(1.0), Some(1.0)]
    );
}

#[test]
fn a_new_slot_owes_its_value_from_now_whatever_its_delay() {
    let start = Instant::now();
    for delay in [-200.0, 0.0, 450.0] {
        let mut slot = Slot::new(
            "/volume".into(),
            1.0,
            settings(SendMode::Continuous, 0.5, delay),
            start,
        );
        assert_eq!(
            wants(&slot, &[]),
            [Want::Frame {
                from: start,
                until: None
            }]
        );
        assert_eq!(sends(&mut slot, start, &[0, 20], &[]), [Some(1.0), None]);
    }
}

#[test]
fn typed_values_keep_their_wire_type_and_threshold_in_delayed_history() {
    let now = Instant::now();
    let mut slot = Slot::quiet(
        "/value".into(),
        SendValue::Int(16_777_217),
        Settings {
            mode: SendMode::Continuous,
            thresholds: Arc::from([10.0]),
            delay_ms: 100.0,
        },
        now,
    );
    slot.start(now);
    let pick = slot.prepare(ms(now, 100), FRAME, &[], false).pick.unwrap();
    assert_eq!(pick.value, SendValues::from(SendValue::Int(16_777_217)));
    slot.sent(pick.value);
    slot.edit(SendValue::Int(16_777_218), ms(now, 110));
    assert!(slot.prepare(ms(now, 210), FRAME, &[], false).pick.is_none());
    let pick = slot.prepare(ms(now, 230), FRAME, &[], false).pick.unwrap();
    assert_eq!(pick.value, SendValues::from(SendValue::Int(16_777_218)));
    slot.sent(pick.value);
    slot.edit(SendValue::Float(16_777_218.0), ms(now, 240));
    assert!(slot.prepare(ms(now, 339), FRAME, &[], false).pick.is_none());
    // A wire type change is a change even when its numeric distance is zero.
    let pick = slot.prepare(ms(now, 340), FRAME, &[], false).pick.unwrap();
    assert_eq!(pick.value, SendValues::from(SendValue::Float(16_777_218.0)));
}

fn values(values: &[SendValue]) -> SendValues {
    SendValues(values.into())
}

#[test]
fn thresholds_compare_each_changed_field_in_sent_units() {
    let now = Instant::now();
    let initial = values(&[SendValue::Float(0.0), SendValue::Float(10.0)]);
    let mut slot = Slot::new(
        "/point".into(),
        initial.clone(),
        Settings {
            mode: SendMode::Continuous,
            thresholds: Arc::from([0.0, 0.5]),
            delay_ms: 0.0,
        },
        now,
    );
    slot.sent(initial);
    let small = values(&[SendValue::Float(0.0), SendValue::Float(10.2)]);
    slot.edit(small.clone(), ms(now, 1));
    // An unchanged zero-Threshold field must not defeat another field's hold.
    assert!(slot.prepare(ms(now, 20), FRAME, &[], false).pick.is_none());
    let pick = slot.prepare(ms(now, 40), FRAME, &[], false).pick.unwrap();
    assert!(pick.value.same(&small));
    slot.sent(pick.value);
    let changed = values(&[SendValue::Float(0.2), SendValue::Float(10.3)]);
    slot.edit(changed.clone(), ms(now, 41));
    let pick = slot.prepare(ms(now, 60), FRAME, &[], false).pick.unwrap();
    assert!(pick.value.same(&changed));
}

#[test]
fn arity_types_strings_and_float_bits_are_wire_changes() {
    let float = values(&[SendValue::Float(0.0)]);
    let negative_zero = values(&[SendValue::Float(-0.0)]);
    assert!(!float.same(&negative_zero));
    for other in [
        values(&[SendValue::Int(0)]),
        values(&[SendValue::Bool(false)]),
        values(&[SendValue::Float(0.0), SendValue::Float(0.0)]),
    ] {
        assert!(!float.same(&other));
        assert!(!float.holds(&other, &[100.0]));
    }
    let a = values(&[SendValue::String("a".into())]);
    let b = values(&[SendValue::String("b".into())]);
    assert!(a.same(&a.clone()));
    assert!(!a.holds(&b, &[100.0]));
    assert!(!values(&[SendValue::Bool(true)]).holds(&values(&[SendValue::Bool(false)]), &[0.0]));
}

#[test]
fn a_thousand_one_field_edits_share_storage_through_prepare_and_sent() {
    let now = Instant::now();
    let mut slot = slot(now, continuous());
    let thresholds = slot.settings.thresholds.clone();
    for edit in 1..=1000 {
        let value = SendValues::from((edit + 1) as f32);
        let at = ms(now, edit * 20);
        slot.edit(value.clone(), at);
        assert!(Arc::ptr_eq(
            value.0.shared(),
            slot.history[0].value.0.shared()
        ));
        let pick = slot.prepare(at, FRAME, &[], false).pick.unwrap();
        assert!(Arc::ptr_eq(value.0.shared(), pick.value.0.shared()));
        assert_eq!(Arc::strong_count(value.0.shared()), 3);
        slot.sent(pick.value);
        assert!(Arc::ptr_eq(
            value.0.shared(),
            slot.last_sent.as_ref().unwrap().0.shared()
        ));
        assert_eq!(Arc::strong_count(value.0.shared()), 3);
        assert!(Arc::ptr_eq(&thresholds, &slot.settings.thresholds));
        assert_eq!(slot.history_len(), 1);
    }
}

#[test]
fn transport_start_keeps_the_automation_sample_it_resends() {
    let start = Instant::now();
    let mut slot = slot(start, continuous());
    slot.sample(SendValues::from(0.5), start);
    slot.start(start);
    assert!(slot.sampled(start));
    assert!(slot.value().same(&SendValues::from(0.5)));
    slot.edit(0.25, ms(start, 1));
    assert!(!slot.sampled(start));
}
