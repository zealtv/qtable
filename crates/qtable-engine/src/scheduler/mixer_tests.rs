//! Activator and Solo silence a playing track without stopping it (rules 1, 2 and 6 of the Live solo and mute table).
use super::tests::project;
use super::*;
use crate::edit::ProjectEdit;
use crate::model::*;

const MS: fn(u64) -> Duration = Duration::from_millis;

struct Rig {
    start: Instant,
    clock: InternalClock,
    p: Planner,
}

/// Two identical looping tracks, both playing from beat zero (500 ms per beat).
fn rig() -> Rig {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut project = project(0.0, true);
    let mut second = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(2));
    for (slot, clip) in second
        .slots
        .iter_mut()
        .filter_map(|s| s.clip.as_mut())
        .enumerate()
    {
        clip.id = ClipId(10 + slot as u64);
    }
    project.tracks.push(second);
    let mut p = Planner::new(project, MS(20)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(2), 0, BeatTime::ZERO).unwrap();
    Rig { start, clock, p }
}

impl Rig {
    fn run_to(&mut self, ms: u64) {
        let at = self.start + MS(ms);
        for event in self.p.plan(&self.clock, at) {
            if event.at <= at {
                self.p.consume(&event);
            }
        }
    }

    fn edit(&mut self, ms: u64, edit: ProjectEdit) {
        let mut project = (*self.p.project).clone();
        crate::edit::apply(&mut project, edit).unwrap();
        self.p
            .update_project(project, &self.clock, self.start + MS(ms))
            .unwrap();
    }

    /// Sends planned at `ms` within `window` ms, as (track, ms after start).
    fn sends(&self, ms: u64, window: u64) -> Vec<(u64, u128)> {
        let mut p = self.p.clone();
        p.lookahead = MS(window);
        p.plan(&self.clock, self.start + MS(ms))
            .iter()
            .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
            .filter_map(|e| {
                let voice = self.p.voices.iter().find(|v| v.id == e.voice)?;
                Some((
                    voice.track.0,
                    e.at.saturating_duration_since(self.start).as_millis(),
                ))
            })
            .collect()
    }

    fn tracks_sending(&self, ms: u64) -> Vec<u64> {
        let mut tracks: Vec<_> = self.sends(ms, 600).into_iter().map(|s| s.0).collect();
        tracks.sort();
        tracks.dedup();
        tracks
    }
}

fn set_active(track: u64, active: bool) -> ProjectEdit {
    ProjectEdit::SetActive {
        strips: vec![TrackId(track)],
        active,
    }
}

fn set_solo(track: u64, solo: bool) -> ProjectEdit {
    ProjectEdit::SetSolo {
        strips: vec![TrackId(track)],
        solo,
        exclusive: true,
    }
}

#[test]
fn an_inactive_track_keeps_playing_silently_and_resumes_without_backlog() {
    let mut r = rig();
    r.run_to(1000);
    r.edit(1000, set_active(1, false));
    assert_eq!(r.tracks_sending(1000), vec![2]);
    r.run_to(5000);
    assert_eq!(r.p.voices.len(), 2, "the silenced clip is still playing");
    r.edit(5000, set_active(1, true));
    let sends = r.sends(5000, 600);
    assert!(sends.contains(&(1, 5000)), "{sends:?}");
    assert!(
        sends.iter().all(|&(_, at)| at >= 5000),
        "backlog: {sends:?}"
    );
}

#[test]
fn solo_silences_the_other_tracks_and_unsolo_restores_them() {
    let mut r = rig();
    r.run_to(1000);
    r.edit(1000, set_solo(2, true));
    assert_eq!(r.tracks_sending(1000), vec![2]);
    r.run_to(3000);
    r.edit(3000, set_solo(2, false));
    assert_eq!(r.tracks_sending(3000), vec![1, 2]);
}

#[test]
fn unsolo_leaves_an_inactive_track_silent() {
    let mut r = rig();
    r.edit(0, set_active(1, false));
    r.edit(0, set_solo(2, true));
    r.run_to(1000);
    r.edit(1000, set_solo(2, false));
    assert_eq!(r.tracks_sending(1000), vec![2]);
}

#[test]
fn output_solo_keeps_silenced_voices_running_and_resumes_without_backlog() {
    let mut r = rig();
    let mut project = (*r.p.project).clone();
    let first_output = project.outputs[0].id;
    project
        .outputs
        .push(Track::new_output(TrackId(299), "Other", "127.0.0.1", 9001));
    project.tracks[1].route = Route::single(TrackId(299));
    r.p.update_project(project, &r.clock, r.start).unwrap();
    r.run_to(1000);
    r.edit(
        1000,
        ProjectEdit::SetOutputSolo {
            output: first_output,
            solo: true,
            exclusive: true,
        },
    );
    assert_eq!(r.tracks_sending(1000), vec![1]);
    r.run_to(5000);
    assert_eq!(r.p.voices.len(), 2);
    r.edit(
        5000,
        ProjectEdit::SetOutputSolo {
            output: first_output,
            solo: false,
            exclusive: true,
        },
    );
    assert_eq!(r.tracks_sending(5000), vec![1, 2]);
    assert!(r.sends(5000, 600).iter().all(|&(_, at)| at >= 5000));
}
