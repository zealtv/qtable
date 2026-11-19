//! Tracks, group tracks and outputs draw ids from one counter, so the later flip of outputs
//! to tracks renames types without colliding ids.
use super::*;

impl Project {
    /// The next free id for a track, group track or output.
    pub fn next_strip_id(&self) -> u64 {
        let tracks = self.tracks.iter().map(|t| t.id.0);
        let outputs = self.outputs.iter().map(|o| o.id.0);
        tracks
            .chain(outputs)
            .chain([self.main.id.0])
            .max()
            .map_or(1, |max| max + 1)
    }

    /// The next free id for a clip.
    pub fn next_clip_id(&self) -> ClipId {
        let clips = self.tracks.iter().flat_map(|t| &t.slots);
        ClipId(
            clips
                .filter_map(|s| s.clip.as_ref().map(|c| c.id.0))
                .max()
                .unwrap_or(0)
                + 1,
        )
    }

    /// Give each output whose id equals a track id a fresh id above every id, in file order,
    /// and follow it in every route. Old files numbered outputs and tracks apart.
    pub fn renumber_colliding_outputs(&mut self) {
        let track_ids: HashSet<u64> = self.tracks.iter().map(|t| t.id.0).collect();
        let mut next = self.next_strip_id();
        let mut moved = Vec::new();
        for output in &mut self.outputs {
            if track_ids.contains(&output.id.0) {
                moved.push((output.id, TrackId(next)));
                output.id = TrackId(next);
                next += 1;
            }
        }
        for track in &mut self.tracks {
            if let Route::Outputs(ids) = &mut track.route {
                for id in ids.iter_mut() {
                    // A route names an output by its old id; only ids that moved change.
                    if let Some((_, new)) = moved.iter().find(|(old, _)| old == id) {
                        *id = *new;
                    }
                }
            }
        }
    }

    pub(super) fn validate_strip_ids(&self) -> ModelResult<()> {
        let track_ids: HashSet<u64> = self.tracks.iter().map(|t| t.id.0).collect();
        require(
            self.outputs.iter().all(|o| !track_ids.contains(&o.id.0)),
            "an output ID must differ from every track ID",
        )?;
        require(
            !self
                .tracks
                .iter()
                .chain(&self.outputs)
                .any(|t| t.id == self.main.id),
            "Main ID must differ from every other track ID",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let mut p = Project::default();
        p.tracks.push(Track::new(TrackId(1), "1", 0));
        p.tracks.push(Track::new(TrackId(2), "2", 0));
        p.tracks[0].route = Route::Outputs(vec![TrackId(1), TrackId(7)]);
        p.tracks[1].route = Route::single(TrackId(2));
        for id in [1, 2, 7] {
            p.outputs
                .push(Track::new_output(TrackId(id), format!("o{id}"), "h", 1));
        }
        p
    }

    #[test]
    fn next_id_follows_every_track_and_output() {
        assert_eq!(Project::default().next_strip_id(), 1);
        assert_eq!(project().next_strip_id(), 8);
    }

    #[test]
    fn next_clip_id_follows_every_clip() {
        let mut p = project();
        assert_eq!(p.next_clip_id(), ClipId(1));
        p.tracks[0].slots.push(Slot {
            clip: Some(Clip::new(ClipId(4), "c", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap()),
            ..Slot::default()
        });
        assert_eq!(p.next_clip_id(), ClipId(5));
    }

    #[test]
    fn colliding_outputs_move_above_every_id_and_routes_follow() {
        let mut p = project();
        assert!(p.validate_strip_ids().is_err());
        p.renumber_colliding_outputs();
        p.validate_strip_ids().unwrap();
        let ids: Vec<_> = p.outputs.iter().map(|o| o.id.0).collect();
        assert_eq!(ids, [8, 9, 7]); // file order; the non-colliding 7 keeps its id
        assert_eq!(
            p.tracks[0].route,
            Route::Outputs(vec![TrackId(8), TrackId(7)])
        );
        assert_eq!(p.tracks[1].route, Route::single(TrackId(9)));
    }

    #[test]
    fn no_collision_changes_nothing() {
        let mut p = project();
        p.renumber_colliding_outputs();
        let before = p.clone();
        p.renumber_colliding_outputs();
        assert_eq!(p, before);
    }
}
