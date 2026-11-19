//! Manual event hits, timed independently of transport and clip voices.
use super::*;
use crate::model::{Event, EventRef, FieldDef, Track};

pub(super) struct Shot {
    pub at: Instant,
    pub track: TrackId,
    pub output: TrackId,
    pub event: EventRef,
    pub packet: OscPacket,
}

#[derive(Default)]
pub(super) struct Queue(VecDeque<Shot>);

impl Queue {
    /// Capture current field values, paths, recipients and delays when the command arrives.
    pub fn fire(&mut self, project: &Project, track: TrackId, event: &EventRef, now: Instant) {
        if !project.track_sends(track) {
            return;
        }
        let Some(piece) = project.event_path(track, event) else {
            return;
        };
        let Some((_, definition)) = project.track(track).and_then(|t| t.event_def(event)) else {
            return;
        };
        let hit = Event {
            position: BeatTime::ZERO,
            definition: event.clone(),
            arguments: definition
                .fields
                .iter()
                .map(FieldDef::current_argument)
                .collect(),
        };
        for output in project.routed_outputs(track).filter(|o| o.mixer.active) {
            let Some(address) = project.resolve_event_address(track, output.id, &piece) else {
                continue;
            };
            let offset = project.send_delay_ms(track, output.id);
            let Some(at) = delay::shift(now, offset.max(0.0)) else {
                continue;
            };
            let index = self.0.partition_point(|shot| shot.at <= at);
            self.0.insert(
                index,
                Shot {
                    at,
                    track,
                    output: output.id,
                    event: event.clone(),
                    packet: event_packet(&hit, &address),
                },
            );
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.0.front().map(|shot| shot.at)
    }

    /// Every queued hit's due instant, earliest first.
    pub fn due_times(&self) -> impl Iterator<Item = Instant> + '_ {
        self.0.iter().map(|shot| shot.at)
    }

    /// Cancel removed sources and recipients before a later edit can reuse their ids, and hits
    /// whose valve closed: a quick mute and unmute must not resurrect an old press.
    pub fn prune(&mut self, project: &Project) {
        self.0.retain(|shot| {
            project
                .track(shot.track)
                .and_then(|track| track.event_def(&shot.event))
                .is_some()
                && project.track_sends(shot.track)
                && project
                    .destination(shot.output)
                    .is_some_and(Track::accepts_sends)
        });
    }

    /// Clip actions win ties; otherwise dispatch the earliest due one-shot.
    pub fn pop_due(&mut self, now: Instant, clip_at: Option<Instant>) -> Option<Shot> {
        let at = self.deadline()?;
        if at > now || clip_at.is_some_and(|clip| clip <= at) {
            return None;
        }
        self.0.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_orders_fanout_keeps_equal_instants_and_captures_packets() {
        let mut project = super::super::tests::project(200.0, true);
        let track = project.tracks[0].id;
        let event = project.tracks[0].slots[0].clip.as_ref().unwrap().events[0]
            .definition
            .clone();
        let first_output = project.outputs[0].id;
        let mut other = project.outputs[0].clone();
        other.id = TrackId(999);
        other.delay_ms = -500.0;
        project.outputs.push(other);
        project.tracks[0].route = crate::model::Route::Outputs(vec![first_output, TrackId(999)]);
        let now = Instant::now();
        let mut queue = Queue::default();
        queue.fire(&project, track, &event, now);
        queue.fire(&project, track, &event, now);
        assert_eq!(queue.deadline(), Some(now));
        assert!(queue.pop_due(now, Some(now)).is_none(), "clips win ties");
        for _ in 0..2 {
            let shot = queue.pop_due(now, None).unwrap();
            assert_eq!(shot.output, TrackId(999));
            assert_eq!(shot.at, now);
        }
        let at = now + Duration::from_millis(200);
        assert_eq!(queue.deadline(), Some(at));
        assert!(queue.pop_due(at - Duration::from_nanos(1), None).is_none());
        project.tracks[0].event_def_mut(&event).unwrap().path =
            crate::model::OscAddress::Absolute("/changed".into());
        project.tracks[0].delay_ms = 900.0;
        project.tracks[0].devices[0].active = false;
        project.tracks[0].route = crate::model::Route::Outputs(vec![]);
        queue.prune(&project);
        for _ in 0..2 {
            let shot = queue.pop_due(at, None).unwrap();
            assert_eq!(shot.output, first_output);
            assert_eq!(shot.at, at);
            let OscPacket::Message(message) = shot.packet else {
                panic!("one message")
            };
            assert_ne!(message.addr, "/changed");
        }
        assert!(queue.deadline().is_none());
    }

    #[test]
    fn prune_cancels_hits_whose_source_or_destination_closed_and_reopening_keeps_them_gone() {
        let mut project = super::super::tests::project(200.0, true);
        let track = project.tracks[0].id;
        let event = project.tracks[0].slots[0].clip.as_ref().unwrap().events[0]
            .definition
            .clone();
        let first_output = project.outputs[0].id;
        let mut other = project.outputs[0].clone();
        other.id = TrackId(999);
        project.outputs.push(other);
        project.tracks[0].route = crate::model::Route::Outputs(vec![first_output, TrackId(999)]);
        let now = Instant::now();
        let at = now + Duration::from_millis(200);
        let mut queue = Queue::default();
        let close: [fn(&mut Project); 3] = [
            |p| p.tracks[0].mixer.active = false,
            |p| {
                let mut soloed = Track::new(TrackId(2), "Solo", p.scenes.len());
                soloed.mixer.solo = true;
                p.tracks.push(soloed);
            },
            |p| p.outputs[0].mixer.active = false,
        ];
        for (case, close) in close.into_iter().enumerate() {
            queue.fire(&project, track, &event, now);
            let mut closed = project.clone();
            close(&mut closed);
            queue.prune(&closed);
            // Reopening at once finds the cancelled hits gone.
            queue.prune(&project);
            if case == 2 {
                // Only the closed destination's hit goes; the other output keeps its own.
                assert_eq!(queue.pop_due(at, None).unwrap().output, TrackId(999));
            }
            assert!(queue.deadline().is_none(), "case {case}");
        }
    }

    #[test]
    fn prune_cancels_removed_tracks_and_only_the_removed_fanout_recipient() {
        let mut project = super::super::tests::project(200.0, true);
        let track = project.tracks[0].id;
        let event = project.tracks[0].slots[0].clip.as_ref().unwrap().events[0]
            .definition
            .clone();
        let first_output = project.outputs[0].id;
        let mut other = project.outputs[0].clone();
        other.id = TrackId(999);
        project.outputs.push(other);
        project.tracks[0].route = crate::model::Route::Outputs(vec![first_output, TrackId(999)]);
        let now = Instant::now();
        let mut queue = Queue::default();
        queue.fire(&project, track, &event, now);
        project.outputs.remove(0);
        queue.prune(&project);
        let at = now + Duration::from_millis(200);
        assert_eq!(queue.deadline(), Some(at));
        assert_eq!(queue.pop_due(at, None).unwrap().output, TrackId(999));
        assert!(queue.deadline().is_none());

        queue.fire(&project, track, &event, now);
        let removed = project.tracks.remove(0);
        queue.prune(&project);
        project.tracks.push(removed);
        queue.prune(&project);
        assert!(queue.deadline().is_none());
    }
}
