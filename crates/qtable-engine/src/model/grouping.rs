//! Track lookups over one list that holds clip tracks and group tracks.
//!
//! A group track precedes its members in `Project.tracks`, as in Live's
//! `Song.tracks`. Code that means "tracks that hold clips" calls `clip_tracks`.
use super::*;

impl Project {
    /// Any track by id: a clip track, a group track or an output.
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks
            .iter()
            .chain(&self.outputs)
            .chain([&self.main])
            .find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .chain(self.outputs.iter_mut())
            .chain([&mut self.main])
            .find(|t| t.id == id)
    }

    /// LOM `Track.clip_slots[i]`: the slot at `(track, slot)`.
    pub fn clip_slot(&self, track: TrackId, slot: usize) -> Option<&Slot> {
        self.track(track)?.clip_slot(slot)
    }

    pub fn clip_slot_mut(&mut self, track: TrackId, slot: usize) -> Option<&mut Slot> {
        self.track_mut(track)?.slots.get_mut(slot)
    }

    /// LOM `ClipSlot.clip`: the clip in the slot at `(track, slot)`, if any.
    pub fn clip(&self, track: TrackId, slot: usize) -> Option<&Clip> {
        self.track(track)?.clip(slot)
    }

    pub fn clip_mut(&mut self, track: TrackId, slot: usize) -> Option<&mut Clip> {
        self.clip_slot_mut(track, slot)?.clip.as_mut()
    }

    /// Tracks that hold clips: scene launch, stop-all, tails and activity act on these.
    pub fn clip_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| !t.is_group())
    }

    /// Group tracks in project order.
    pub fn group_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| t.is_group())
    }

    /// LOM `group_track`: the group track a track belongs to.
    pub fn group_of(&self, track: TrackId) -> Option<&Track> {
        self.track(self.track(track)?.group?)
    }

    /// A group's member tracks in project order; empty for an unknown group.
    pub fn members(&self, group: TrackId) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(move |t| t.group == Some(group))
    }

    /// Put `group` into the project as a group track just before `members[0]`, and make
    /// `members` (adjacent, in project order, not yet grouped) its members.
    pub fn insert_group(&mut self, mut group: Track, members: &[TrackId]) {
        group.kind = TrackKind::Group;
        group.group = None;
        let at = self
            .tracks
            .iter()
            .position(|t| Some(&t.id) == members.first())
            .unwrap_or(self.tracks.len());
        for track in self.tracks.iter_mut().filter(|t| members.contains(&t.id)) {
            track.group = Some(group.id);
        }
        self.tracks.insert(at, group);
    }

    /// A group track's member ids in project order; None when `group` is not a group track.
    pub fn group_member_ids(&self, group: TrackId) -> Option<Vec<TrackId>> {
        self.track(group)
            .filter(|t| t.is_group())
            .map(|_| self.members(group).map(|t| t.id).collect())
    }
}

impl Track {
    /// LOM `Track.clip_slots[i]`, for a caller that already holds the track.
    pub fn clip_slot(&self, slot: usize) -> Option<&Slot> {
        self.slots.get(slot)
    }

    /// LOM `ClipSlot.clip` in this track's slot `slot`, if any.
    pub fn clip(&self, slot: usize) -> Option<&Clip> {
        self.clip_slot(slot)?.clip.as_ref()
    }
}

/// `project[id]` for tests and tools that know the track exists; it panics otherwise.
impl std::ops::Index<TrackId> for Project {
    type Output = Track;
    fn index(&self, id: TrackId) -> &Track {
        self.track(id).expect("track id exists in the project")
    }
}

impl std::ops::IndexMut<TrackId> for Project {
    fn index_mut(&mut self, id: TrackId) -> &mut Track {
        self.track_mut(id).expect("track id exists in the project")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tracks 1, 5 (a group), 2 and 3 (its members), 4.
    fn project() -> Project {
        let mut p = Project::default();
        p.tracks.push(Track::new(TrackId(1), "1", 0));
        p.tracks.push(Track::new_group(TrackId(5), "G", 0));
        for id in [2, 3] {
            let mut t = Track::new(TrackId(id), id.to_string(), 0);
            t.group = Some(TrackId(5));
            p.tracks.push(t);
        }
        p.tracks.push(Track::new(TrackId(4), "4", 0));
        p
    }

    #[test]
    fn lookups_find_tracks_groups_and_members() {
        let mut p = project();
        assert_eq!(p.track(TrackId(2)).map(|t| t.name.as_str()), Some("2"));
        assert!(p.track(TrackId(9)).is_none());
        p.track_mut(TrackId(1)).unwrap().name = "one".into();
        assert_eq!(p.tracks[0].name, "one");
        let members: Vec<_> = p.members(TrackId(5)).map(|t| t.id).collect();
        assert_eq!(members, [TrackId(2), TrackId(3)]);
        assert_eq!(p.members(TrackId(7)).count(), 0);
        assert_eq!(p.group_of(TrackId(3)).map(|g| g.id), Some(TrackId(5)));
        assert!(p.group_of(TrackId(1)).is_none());
        assert_eq!(p.clip_tracks().count(), 4);
        assert_eq!(p.group_tracks().count(), 1);
    }
}
