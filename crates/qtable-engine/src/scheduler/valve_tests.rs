//! track-valve-build (2026-10-22): a muted or soloed-away source sends no parameter OSC from the
//! command's application on, while its automation keeps moving and other sources on the same
//! output keep theirs; reopening owes every pair its current sample on ordinary frames, whatever
//! Threshold says, under Budget, at the recipient's delayed time, with no replayed jumps.
//! Simulated worker (`cluster_sim`), every frame's values recorded.
use super::cluster_sim::*;
use super::*;
use crate::model::*;

/// The ramp's period (one 3840-tick bar at 120 BPM): a sent ramp value × this is its sample time.
const RAMP: f32 = 2000.0;
const RAMP_TRACK: TrackId = TrackId(500);
/// One of the 16 Every Frame volumes on A (`/parameter/v0`).
const VOLUME_TRACK: TrackId = TrackId(11);

fn ramp(delay_ms: f64, threshold: f64) -> Track {
    let mut track = ramp_track(3840);
    track.delay_ms = delay_ms;
    let target = track.devices[0].target_mut().unwrap();
    for parameter in &mut target.parameters {
        parameter.fields[0].threshold = threshold;
    }
    track
}

fn sim(tracks: Vec<Track>, budget: Option<u32>) -> Sim {
    let mut project = project(120.0, &[A], tracks);
    if let Some(budget) = budget {
        let stage = &mut project.outputs[0].output.as_mut().unwrap().stage;
        stage.budget_on = true;
        stage.budget = budget;
    }
    let mut sim = Sim::new(project, MS(20), Duration::ZERO).launched();
    sim.record_values = true;
    sim
}

/// Post `edit` to the worker's current project, run until `until` after the start, and return
/// when it was applied.
fn post(sim: &mut Sim, edit: impl FnOnce(&mut Project), until: Duration) -> Instant {
    let mut project = (*sim.planner.project).clone();
    edit(&mut project);
    let applied = sim.log.len();
    sim.send(SchedulerCommand::UpdateProject(project));
    sim.run_until(sim.start + until);
    sim.log[applied].applied
}

fn mute(track: TrackId, active: bool) -> impl FnOnce(&mut Project) {
    move |project| {
        project
            .tracks
            .iter_mut()
            .find(|t| t.id == track)
            .unwrap()
            .mixer
            .active = active
    }
}

/// Every frame on A that began at or after `from`, with what it sent.
fn frames_from(sim: &Sim, from: Instant) -> Vec<(Frame, &[(String, f32)])> {
    sim.frames
        .iter()
        .zip(&sim.values)
        .filter(|(frame, _)| frame.output == A && frame.start >= from)
        .map(|(frame, values)| (*frame, values.as_slice()))
        .collect()
}

fn sends(values: &[(String, f32)], prefix: &str) -> usize {
    values.iter().filter(|(a, _)| a.starts_with(prefix)).count()
}

/// The first value each ramp parameter sent from `from`, with its frame's tick.
fn first_ramp_sends(sim: &Sim, from: Instant) -> Vec<(String, f32, Instant)> {
    let mut first: Vec<(String, f32, Instant)> = Vec::new();
    for (frame, values) in frames_from(sim, from) {
        for (address, value) in values {
            if address.starts_with("/parameter/p") && !first.iter().any(|(a, ..)| a == address) {
                first.push((address.clone(), *value, frame.tick));
            }
        }
    }
    first
}

fn ramp_value(sim: &Sim, at: Instant) -> Option<f64> {
    sim.stages
        .automation_values(at)
        .into_iter()
        .find(|value| {
            value.field.parameter.track == RAMP_TRACK && value.field.parameter.key.0 == "p0"
        })
        .map(|value| value.value)
}

#[test]
fn closing_silences_only_that_source_from_application_while_its_automation_moves() {
    let mut tracks = vec![ramp(0.0, 0.0)];
    tracks.extend(every_frame_volumes());
    let mut sim = sim(tracks, None);
    sim.run_until(sim.start + MS(300));
    assert!(
        frames_from(&sim, sim.start)
            .iter()
            .any(|(_, values)| sends(values, "/parameter/p") == 16)
    );
    let closed = post(&mut sim, mute(RAMP_TRACK, false), MS(310));
    let closed = closed.max(post(&mut sim, mute(VOLUME_TRACK, false), MS(800)));
    let after = frames_from(&sim, closed);
    assert!(after.len() > 25, "{}", after.len());
    for (frame, values) in after {
        assert_eq!(sends(values, "/parameter/p"), 0, "{frame:?}");
        assert_eq!(sends(values, "/parameter/v0"), 0, "{frame:?}");
        // The other 15 Every Frame volumes share the output and keep every frame.
        assert_eq!(sends(values, "/parameter/v"), 15, "{frame:?}");
    }
    let (early, late) = (sim.start + MS(500), sim.start + MS(800));
    assert!(ramp_value(&sim, late) > ramp_value(&sim, early));
}

#[test]
fn reopening_sends_every_pair_its_current_sample_on_the_next_frame_without_replays() {
    // A huge Threshold can't hold back the resynchronisation of a fresh pair.
    for threshold in [0.0, 0.5] {
        let mut sim = sim(vec![ramp(0.0, threshold)], None);
        sim.run_until(sim.start + MS(200));
        post(&mut sim, mute(RAMP_TRACK, false), MS(700));
        let reopened = post(&mut sim, mute(RAMP_TRACK, true), MS(800));
        let first = first_ramp_sends(&sim, reopened);
        assert_eq!(first.len(), 16, "{first:?}");
        for (address, value, tick) in &first {
            assert!(
                (reopened..=reopened + PERIOD).contains(tick),
                "{address} at {tick:?}"
            );
            // The sample at its own tick: never the saved base (0) or a stale value.
            let ms = (*tick - sim.start).as_secs_f32() * 1000.0;
            assert!(
                (value * RAMP - ms).abs() < 0.01,
                "{address} {value} at {ms}"
            );
        }
        // Nothing from the missed launch or ramp is replayed: no exact jump before the wrap.
        assert!(
            frames_from(&sim, reopened)
                .iter()
                .all(|(frame, _)| !frame.start_send),
        );
    }
}

#[test]
fn reopening_samples_at_the_recipients_delayed_time_without_waiting_for_the_delay() {
    let mut sim = sim(vec![ramp(300.0, 0.0)], None);
    sim.run_until(sim.start + MS(500));
    post(&mut sim, mute(RAMP_TRACK, false), MS(700));
    let reopened = post(&mut sim, mute(RAMP_TRACK, true), MS(1200));
    let first = first_ramp_sends(&sim, reopened);
    assert_eq!(first.len(), 16, "{first:?}");
    for (address, value, tick) in &first {
        assert!(
            (reopened..=reopened + PERIOD).contains(tick),
            "{address} at {tick:?}"
        );
        let musical = (*tick - sim.start).as_secs_f32() * 1000.0 - 300.0;
        assert!(
            (value * RAMP - musical).abs() < 0.01,
            "{address} {value} at {musical} reopened {:?}",
            reopened - sim.start
        );
    }
}

#[test]
fn reopening_under_budget_spreads_current_samples_over_frames() {
    // Two messages a frame at 60 Hz, with a bucket of two frames' worth.
    let mut sim = sim(vec![ramp(0.0, 0.0)], Some(120));
    sim.run_until(sim.start + MS(200));
    post(&mut sim, mute(RAMP_TRACK, false), MS(700));
    let reopened = post(&mut sim, mute(RAMP_TRACK, true), MS(1200));
    for (frame, values) in frames_from(&sim, reopened) {
        assert!(values.len() <= 4, "{frame:?} {values:?}");
    }
    let first = first_ramp_sends(&sim, reopened);
    assert_eq!(first.len(), 16, "{first:?}");
    let latest = first.iter().map(|(_, _, tick)| *tick).max().unwrap();
    assert!(latest > reopened + PERIOD * 4, "the Budget spread them");
    for (address, value, tick) in &first {
        let ms = (*tick - sim.start).as_secs_f32() * 1000.0;
        assert!(
            (value * RAMP - ms).abs() < 0.01,
            "{address} {value} at {ms}"
        );
    }
}

#[test]
fn parameter_override_and_reenable_work_while_the_track_valve_is_closed() {
    let mut sim = sim(vec![ramp(0.0, 0.0)], None);
    sim.run_until(sim.start + MS(200));
    let closed = post(&mut sim, mute(RAMP_TRACK, false), MS(400));
    let field = ArgumentRef {
        device: DeviceRef::Chain(DeviceId(500)),
        parameter: ParameterKey("p0".into()),
        field: FieldKey("value".into()),
    }
    .on(RAMP_TRACK);
    let mut edited = (*sim.planner.project).clone();
    // The unchanged saved base must still override a moving curve while muted.
    let base = edited.field(&field).unwrap().value;
    edited
        .set_parameter_values(&field.parameter, vec![(field.field.clone(), base)])
        .unwrap();
    sim.send(SchedulerCommand::UpdateParameters(
        crate::engine::ValueEdit::for_test(edited, vec![field.clone()]),
    ));
    sim.run_until(sim.start + MS(600));
    let values = sim.stages.automation_values(sim.start + MS(600));
    let overridden = values.iter().find(|v| v.field == field).unwrap();
    assert_eq!(overridden.state, AutomationState::Overridden);
    assert_eq!(overridden.value, base);
    assert!(
        values
            .iter()
            .any(|v| v.field != field && v.state == AutomationState::Following)
    );
    sim.send(SchedulerCommand::ReEnableAutomation(Some(field.clone())));
    sim.run_until(sim.start + MS(800));
    assert!(
        sim.stages
            .automation_values(sim.start + MS(800))
            .iter()
            .any(|v| {
                v.field == field && v.state == AutomationState::Following && v.value > base
            })
    );
    assert!(
        frames_from(&sim, closed)
            .iter()
            .all(|(_, values)| values.is_empty())
    );
    let reopened = post(&mut sim, mute(RAMP_TRACK, true), MS(1000));
    assert_eq!(first_ramp_sends(&sim, reopened).len(), 16);
}
