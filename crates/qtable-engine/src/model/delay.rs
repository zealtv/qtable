//! Track Delay along the signal path (2026-10-13).
//!
//! In Live, delay compensation sums along a track's path: its own Track Delay, its group
//! tracks' and Main's. qtable sums the ancestor path before an output's latency.
use super::*;

impl Project {
    /// Shift a source's stream by its signal-path delay and the destination's latency.
    /// Self-targeted controls already include their own latency in the source delay.
    pub fn send_delay_ms(&self, source: TrackId, destination: TrackId) -> f64 {
        self.effective_delay_ms(source)
            + if source == destination {
                0.0
            } else {
                self.output_latency_ms(destination)
            }
    }

    /// Output latency is additional; Main's global delay already belongs to the ancestor walk.
    pub fn output_latency_ms(&self, output: TrackId) -> f64 {
        self.destination(output)
            .filter(|t| t.id != self.main.id)
            .map_or(0.0, |t| t.delay_ms)
    }
    /// Own delay plus every group ancestor and Main, in ms; zero for an unknown track.
    pub fn effective_delay_ms(&self, track: TrackId) -> f64 {
        let Some(own) = self.track(track) else {
            return 0.0;
        };
        own.delay_ms + self.ancestors(track).map(Ancestor::delay_ms).sum::<f64>()
    }
}

impl Track {
    /// A destination accepts sends while configured and active, regardless of Solo.
    pub fn accepts_sends(&self) -> bool {
        self.mixer.active && self.destination().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let mut p = Project {
            tracks: vec![
                Track::new(TrackId(1), "a", 0),
                Track::new(TrackId(2), "b", 0),
            ],
            ..Project::default()
        };
        p.tracks[0].delay_ms = 5.0;
        {
            let mut group = Track::new_group(TrackId(101), "g", p.scenes.len());
            group.delay_ms = -2.0;
            group.mixer = MixerState::default();
            group.colour = Colour::UNSET;
            p.insert_group(group, &[TrackId(1)]);
        }
        p.main.delay_ms = 10.0;
        p
    }

    #[test]
    fn delay_sums_own_group_and_main() {
        let p = project();
        assert_eq!(p.effective_delay_ms(TrackId(1)), 13.0);
        // Not in the group: own + Main only.
        assert_eq!(p.effective_delay_ms(TrackId(2)), 10.0);
        assert_eq!(p.effective_delay_ms(TrackId(9)), 0.0);
    }

    #[test]
    fn send_delay_adds_destination_latency_without_clamping_signed_offsets() {
        let mut p = project();
        let mut output = Track::new_output(TrackId(201), "out", "127.0.0.1", 9000);
        output.delay_ms = 7.0;
        p.outputs.push(output);
        assert_eq!(p.send_delay_ms(TrackId(1), TrackId(201)), 20.0);
        assert_eq!(p.send_delay_ms(TrackId(101), TrackId(201)), 15.0);
        assert_eq!(p.send_delay_ms(TrackId(1), p.main.id), 13.0);
        p.outputs[0].delay_ms = -20.0;
        assert_eq!(p.send_delay_ms(TrackId(1), TrackId(201)), -7.0);
    }

    #[test]
    fn self_targeted_output_and_main_count_their_delay_once() {
        let mut p = project();
        let mut output = Track::new_output(TrackId(201), "out", "127.0.0.1", 9000);
        output.delay_ms = -7.0;
        p.outputs.push(output);
        assert_eq!(p.send_delay_ms(TrackId(201), TrackId(201)), 3.0);
        assert_eq!(p.send_delay_ms(p.main.id, p.main.id), 10.0);
    }

    #[test]
    fn accepts_sends_requires_an_active_configured_destination() {
        let p = project();
        let output = Track::new_output(TrackId(201), "out", "127.0.0.1", 9000);
        assert!(!p.track(TrackId(1)).unwrap().accepts_sends());
        assert!(!p.track(TrackId(101)).unwrap().accepts_sends());
        let destination = output.output.clone();
        for mut track in [p.main, output] {
            for active in [false, true] {
                for configured in [false, true] {
                    for solo in [false, true] {
                        track.mixer.active = active;
                        track.mixer.solo = solo;
                        track.output = if configured {
                            destination.clone()
                        } else {
                            None
                        };
                        assert_eq!(track.accepts_sends(), active && configured);
                    }
                }
            }
        }
    }

    #[test]
    fn old_files_load_with_zero_group_and_main_delay() {
        let p: Project = ron::from_str("Project()").unwrap();
        assert_eq!(p.main.delay_ms, 0.0);
    }
}
