//! Clip lane edits. Path edits and the legacy single removal use event-only indices
//! (`Clip::lane_refs`); list operations address the one mixed `Clip::lanes` list.
use super::{Carried, EditReport};
use crate::model::{EventLane, EventRef, ModelError, OscAddress, Project, Track, TrackId};

/// Stable, track-local identity of a clip lane.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LaneKey {
    Events(EventRef),
    Envelope(crate::model::ArgumentRef),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaneContent {
    Hits(Vec<crate::model::Event>),
    Points(Vec<crate::model::Breakpoint>),
}

/// Which existing envelope points a ReplaceRange write removes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointReplacement {
    /// Time-range edits preserve both edges (D8).
    KeepEdges,
    /// Picked-item edits replace [start, end), including points at start.
    Exact,
}

impl PointReplacement {
    /// Apply a validated point write, also used by client previews. Time-range writes keep
    /// the first arriving value and last outgoing value at each time; an equal pair keeps
    /// only the last point, whose curve shapes the segment leaving it. Exact picked writes
    /// preserve the supplied siblings and their ordinals without coalescing them.
    pub fn replace_points(
        self,
        existing: &mut Vec<crate::model::Breakpoint>,
        range: &std::ops::Range<crate::model::BeatTime>,
        incoming: impl IntoIterator<Item = crate::model::Breakpoint>,
    ) {
        existing.retain(|p| !self.contains(range, p.time));
        existing.extend(incoming);
        // Stable order puts retained edges before the incoming content.
        existing.sort_by_key(|p| p.time);
        if self == Self::Exact {
            return;
        }
        let (mut read, mut write) = (0, 0);
        while read < existing.len() {
            let first = read;
            while read + 1 < existing.len() && existing[read + 1].time == existing[first].time {
                read += 1;
            }
            if existing[first].value != existing[read].value {
                existing[write] = existing[first].clone();
                write += 1;
            }
            existing[write] = existing[read].clone();
            write += 1;
            read += 1;
        }
        existing.truncate(write);
    }

    /// Whether an existing point at this time is removed by the write.
    pub fn contains(
        self,
        range: &std::ops::Range<crate::model::BeatTime>,
        time: crate::model::BeatTime,
    ) -> bool {
        range.contains(&time) && (self == Self::Exact || time > range.start)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaneEdit {
    /// Set an envelope's own markers, or relink it with None, preserving points.
    /// Address envelopes by stable argument identity. To unlink without changing markers,
    /// pass Some(clip.region()); event lanes cannot unlink.
    SetRegion {
        lane: LaneKey,
        region: Option<crate::model::region::Region>,
    },
    /// Replace hits in [start, end), and points according to point_policy, on each
    /// named lane atomically. No points are synthesized.
    /// Incoming content must be sorted and inside [start, end). Missing lanes are
    /// created for valid track-local identities. An empty range is rejected.
    ReplaceRange {
        range: std::ops::Range<crate::model::BeatTime>,
        lanes: Vec<(LaneKey, LaneContent)>,
        point_policy: PointReplacement,
    },
    /// Append a lane for the event at this relative path on the clip's track, creating the event (with
    /// no arguments) in the track's first Target, and that Target, when none matches.
    AddPath(String),
    /// Append a lane for this existing event on the clip's track, preserving its identity
    /// even when another Target has the same path, or the path is absolute.
    AddEvent(EventRef),
    /// Rename the lane's event in place when this lane is its only user, else re-point the
    /// lane at the first event with this path (creating one with the old signature) and
    /// conform its hits. The path takes the current event's kind (relative or absolute).
    SetPath { lane: usize, path: String },
    /// Legacy event-only index: remove this event lane and its hits; keep the definition.
    /// Use RemoveMany for mixed-list indices.
    Remove { lane: usize },
    /// Move a nonempty contiguous range in the mixed list to its final start index,
    /// as MoveTrack does (the destination is measured after removing the range).
    Move {
        from: std::ops::Range<usize>,
        to: usize,
    },
    /// Remove a set of mixed-list indices and their content atomically.
    RemoveMany { lanes: Vec<usize> },
    /// Paste whole lanes and their hits at a mixed-list insertion index. Definitions
    /// travel as for PasteClip. Existing event lanes merge hits; existing envelopes win.
    Paste {
        at: usize,
        lanes: Vec<crate::model::Lane>,
        hits: Vec<crate::model::Event>,
        carried: Vec<Carried>,
    },
}

fn taken(lanes: &[EventRef], reference: &EventRef) -> Result<(), ModelError> {
    if lanes.contains(reference) {
        Err(ModelError("another lane already plays that event".into()))
    } else {
        Ok(())
    }
}

pub(super) fn apply(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    edit: LaneEdit,
) -> Result<EditReport, ModelError> {
    // Legacy event edits keep their event-only indices. All new operations share one
    // implementation over the mixed list, including the legacy single removal.
    match edit {
        LaneEdit::SetRegion { lane, region } => {
            let region = region.map(|r| r.normalised());
            if let Some(region) = region {
                region.validate()?;
            }
            let clip = super::lane_list::get_clip(project, track, slot)?;
            let LaneKey::Envelope(target) = lane else {
                return Err(ModelError("event lanes cannot unlink".into()));
            };
            let envelope = clip
                .lanes
                .iter_mut()
                .filter_map(|lane| match lane {
                    crate::model::Lane::Envelope(envelope) => Some(envelope),
                    _ => None,
                })
                .find(|envelope| envelope.target == target)
                .ok_or_else(|| ModelError("envelope lane not found".into()))?;
            envelope.region = region;
            return Ok(EditReport::default());
        }
        LaneEdit::ReplaceRange {
            range,
            lanes,
            point_policy,
        } => {
            return super::replace_range::apply(project, track, slot, range, lanes, point_policy);
        }
        LaneEdit::Move { from, to } => {
            return super::lane_list::move_lanes(project, track, slot, from, to);
        }
        LaneEdit::RemoveMany { lanes } => {
            return super::lane_list::remove(project, track, slot, lanes);
        }
        LaneEdit::Paste {
            at,
            lanes,
            hits,
            carried,
        } => return super::lane_list::paste(project, track, slot, at, lanes, hits, carried),
        LaneEdit::Remove { lane } => {
            let clip = super::lane_list::get_clip(project, track, slot)?;
            let reference = clip
                .lane_refs()
                .get(lane)
                .cloned()
                .ok_or_else(|| ModelError("lane index out of range".into()))?;
            super::lane_list::declare_events(clip);
            let index = clip
                .lanes
                .iter()
                .position(|l| l.event().is_some_and(|l| l.definition == reference))
                .unwrap();
            return super::lane_list::remove(project, track, slot, vec![index]);
        }
        _ => {}
    }
    let mut report = EditReport::default();
    let device = project.next_device_id();
    let track = project
        .track_mut(track)
        .ok_or_else(|| ModelError("unknown track".into()))?;
    let mut clip = track
        .slots
        .get(slot)
        .ok_or_else(|| ModelError("slot index out of range".into()))?
        .clip
        .clone()
        .ok_or_else(|| ModelError("slot is empty".into()))?;
    let mut lanes = clip.lane_refs();
    let original = lanes.clone();
    let mut repointed = None;
    let out_of_range = || ModelError("lane index out of range".into());
    match edit {
        LaneEdit::AddPath(path) => {
            let path = OscAddress::Relative(path).normalised()?;
            let reference = match track.find_event(&path) {
                Some(reference) => reference,
                None => track.ensure_event(path, &[], || device),
            };
            taken(&lanes, &reference)?;
            lanes.push(reference);
        }
        LaneEdit::AddEvent(reference) => {
            if track.event_def(&reference).is_none() {
                return Err(ModelError("the lane's event isn't on its track".into()));
            }
            taken(&lanes, &reference)?;
            lanes.push(reference);
        }
        LaneEdit::SetPath { lane, path } => {
            let current = lanes.get(lane).ok_or_else(out_of_range)?.clone();
            let (_, definition) = track
                .event_def(&current)
                .ok_or_else(|| ModelError("the lane's event isn't on its track".into()))?;
            let path = match definition.path {
                OscAddress::Relative(_) => OscAddress::Relative(path),
                OscAddress::Absolute(_) => OscAddress::Absolute(path),
            }
            .normalised()?;
            if path == definition.path {
                return Ok(report);
            }
            let signature: Vec<_> = definition.signature().collect();
            // This lane is the event's only user.
            if track.event_uses(&current).0 == 1 && track.find_event(&path).is_none() {
                track.event_def_mut(&current).expect("resolved above").path = path;
                return Ok(report);
            }
            let target = match track.find_event(&path) {
                Some(reference) => reference,
                None => track.ensure_event(path, &signature, || device),
            };
            taken(&lanes, &target)?;
            let (_, definition) = track.event_def(&target).expect("found or added above");
            let signature: Vec<_> = definition.signature().collect();
            let mut converted = 0;
            for event in clip.events.iter_mut().filter(|e| e.definition == current) {
                event.definition = target.clone();
                converted += usize::from(event.conform(&signature));
            }
            report.converted(converted, &signature);
            lanes[lane] = target;
            repointed = Some(current);
        }
        LaneEdit::Remove { .. }
        | LaneEdit::SetRegion { .. }
        | LaneEdit::ReplaceRange { .. }
        | LaneEdit::Move { .. }
        | LaneEdit::RemoveMany { .. }
        | LaneEdit::Paste { .. } => unreachable!("handled above"),
    }
    // Keep envelope positions when an event lane is removed or repointed.
    let mut declared = Vec::new();
    clip.lanes = clip
        .lanes
        .into_iter()
        .filter_map(|lane| match lane {
            crate::model::Lane::Envelope(_) => Some(lane),
            crate::model::Lane::Events(event) => {
                let index = original.iter().position(|r| *r == event.definition)?;
                let definition = if let Some(old) = &repointed {
                    if *old == event.definition {
                        lanes.get(index)?.clone()
                    } else {
                        event.definition
                    }
                } else {
                    event.definition
                };
                if !lanes.contains(&definition) {
                    return None;
                }
                declared.push(definition.clone());
                Some(crate::model::Lane::Events(EventLane { definition }))
            }
        })
        .collect();
    clip.lanes.extend(
        lanes
            .into_iter()
            .filter(|r| !declared.contains(r))
            .map(|definition| crate::model::Lane::Events(EventLane { definition })),
    );
    track.slots[slot].clip = Some(clip);
    if let Some(old) = repointed
        && track.event_uses(&old).0 == 0
        && let Some((_, definition)) = track.event_def(&old)
        && definition.name.is_empty()
        && definition.fields.is_empty()
    {
        let device = track
            .devices
            .iter_mut()
            .find(|d| d.id == old.device)
            .unwrap();
        if let Some(target) = device.target_mut() {
            target.events.retain(|e| e.key != old.key);
        }
    }
    Ok(report)
}

/// Remove matching lanes (declared or derived) and their hits from every clip on a track,
/// reporting what went. Device deletion and Target-entry deletion share this rule.
pub(super) fn remove_matching(
    track: &mut Track,
    matches: impl Fn(&EventRef) -> bool,
) -> EditReport {
    let mut report = EditReport::default();
    for clip in track.slots.iter_mut().filter_map(|slot| slot.clip.as_mut()) {
        let lanes = clip.lane_refs().iter().filter(|r| matches(r)).count();
        let hits = clip.events.len();
        clip.lanes
            .retain(|lane| lane.event().is_none_or(|lane| !matches(&lane.definition)));
        clip.events.retain(|event| !matches(&event.definition));
        report.removed(lanes, hits - clip.events.len());
    }
    report
}

#[cfg(test)]
mod tests {
    use crate::edit::{EditReport, LaneEdit, LaneKey, ProjectEdit, apply};
    use crate::model::*;

    fn relative(a: &str) -> OscAddress {
        OscAddress::Relative(a.into())
    }

    /// Track 1 with two clips: slot 0 hits `/a` (i) twice and `/b` (absolute) once; slot 1
    /// is empty. No lanes are declared, so they are derived.
    fn project() -> Project {
        let mut p = Project {
            scenes: (0..2)
                .map(|i| Scene {
                    id: SceneId(i),
                    name: format!("S{i}"),
                    follow_actions: Default::default(),
                })
                .collect(),
            ..Project::default()
        };
        p.tracks.push(Track::new(TrackId(1), "T", 2));
        let mut clip = Clip::new(ClipId(1), "C", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
        let int = || vec![OscArgument::Int(1)];
        clip.events = vec![
            p.hit(TrackId(1), BeatTime(0), relative("/a"), int()),
            p.hit(
                TrackId(1),
                BeatTime(960),
                OscAddress::Absolute("/b".into()),
                vec![],
            ),
            p.hit(TrackId(1), BeatTime(1920), relative("/a"), int()),
        ]
        .into_iter()
        .map(Option::unwrap)
        .collect();
        let mut empty = clip.clone();
        empty.id = ClipId(2);
        empty.events.clear();
        p.tracks[0].slots[0].clip = Some(clip);
        p.tracks[0].slots[1].clip = Some(empty);
        p.validate().unwrap();
        p
    }

    fn lane(p: &mut Project, slot: usize, edit: LaneEdit) -> Result<EditReport, ModelError> {
        let track = TrackId(1);
        apply(p, ProjectEdit::EditLane { track, slot, edit })
    }

    fn clip(p: &Project, slot: usize) -> &Clip {
        p.tracks[0].slots[slot].clip.as_ref().unwrap()
    }

    fn path(p: &Project, reference: &EventRef) -> OscAddress {
        p.tracks[0].event_def(reference).unwrap().1.path.clone()
    }

    fn events(p: &Project) -> Vec<(OscAddress, Vec<FieldType>)> {
        p.tracks[0]
            .targets()
            .flat_map(|(_, t)| &t.events)
            .map(|e| (e.path.clone(), e.signature().collect()))
            .collect()
    }

    #[test]
    fn derived_lanes_follow_first_appearance_and_declared_lanes_come_first() {
        let mut p = project();
        let lanes = clip(&p, 0).lane_refs();
        assert_eq!(path(&p, &lanes[0]), relative("/a"));
        assert_eq!(path(&p, &lanes[1]), OscAddress::Absolute("/b".into()));
        let definition = p.ensure_event(TrackId(1), relative("/e"), &[]).unwrap();
        p.tracks[0].slots[0].clip.as_mut().unwrap().lanes =
            vec![crate::model::Lane::Events(EventLane { definition })];
        let lanes = clip(&p, 0).lane_refs();
        assert_eq!(path(&p, &lanes[0]), relative("/e"));
        assert_eq!(lanes.len(), 3);
    }

    #[test]
    fn add_event_preserves_identity_and_rejects_duplicate_or_missing_refs_atomically() {
        let mut p = project();
        let reference = clip(&p, 0).events[1].definition.clone();
        assert!(matches!(path(&p, &reference), OscAddress::Absolute(_)));
        let definitions = events(&p);
        lane(&mut p, 1, LaneEdit::AddEvent(reference.clone())).unwrap();
        assert_eq!(clip(&p, 1).lane_refs(), vec![reference.clone()]);
        assert!(clip(&p, 1).events.is_empty());
        assert_eq!(events(&p), definitions);
        let before = p.clone();
        assert!(lane(&mut p, 1, LaneEdit::AddEvent(reference.clone())).is_err());
        assert_eq!(p, before);
        let missing = EventRef {
            device: crate::model::DeviceId(999),
            ..reference
        };
        assert!(lane(&mut p, 1, LaneEdit::AddEvent(missing)).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn text_edits_add_relative_paths_and_preserve_an_absolute_events_kind() {
        let mut p = project();
        lane(&mut p, 1, LaneEdit::AddPath("kick".into())).unwrap();
        let reference = &clip(&p, 1).lanes[0].event().unwrap().definition;
        assert_eq!(path(&p, reference), relative("/kick"));
        lane(
            &mut p,
            0,
            LaneEdit::SetPath {
                lane: 1,
                path: "cue".into(),
            },
        )
        .unwrap();
        assert_eq!(clip_lanes(&p)[1], OscAddress::Absolute("/cue".into()));
    }

    #[test]
    fn add_path_creates_the_target_and_event_or_reuses_one() {
        let mut p = Project::default();
        p.scenes.push(Scene {
            id: SceneId(0),
            name: "S".into(),
            follow_actions: Default::default(),
        });
        p.tracks.push(Track::new(TrackId(1), "T", 1));
        let new = Clip::new(ClipId(1), "C", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
        p.tracks[0].slots[0].clip = Some(new);
        lane(&mut p, 0, LaneEdit::AddPath("kick".into())).unwrap();
        assert_eq!(p.tracks[0].devices.len(), 1);
        assert_eq!(p.tracks[0].devices[0].name, "Target");
        assert_eq!(events(&p), vec![(relative("/kick"), vec![])]);
        assert_eq!(clip_lanes(&p), vec![relative("/kick")]);

        let mut p = project();
        lane(&mut p, 1, LaneEdit::AddPath("/a".into())).unwrap();
        assert_eq!(events(&p).len(), 2);
        let reused = &clip(&p, 1).lanes[0].event().unwrap().definition;
        assert_eq!(*reused, clip(&p, 0).events[0].definition);
        // Also a lane may not play an event another lane of its clip plays.
        let err = lane(&mut p, 1, LaneEdit::AddPath("/a".into())).unwrap_err();
        assert!(err.to_string().contains("another lane already plays"));
        assert!(lane(&mut p, 1, LaneEdit::AddPath("a b".into())).is_err());
    }

    fn clip_lanes(p: &Project) -> Vec<OscAddress> {
        clip(p, 0).lane_refs().iter().map(|r| path(p, r)).collect()
    }

    #[test]
    fn set_path_renames_in_place_when_the_lane_is_the_only_user() {
        let mut p = project();
        let before = clip(&p, 0).clone();
        let edit = |path: &str| LaneEdit::SetPath {
            lane: 0,
            path: path.into(),
        };
        lane(&mut p, 0, edit("kick")).unwrap();
        // The same event, renamed: refs and hits are untouched, no event is added.
        assert_eq!(clip(&p, 0).events, before.events);
        assert_eq!(events(&p)[0], (relative("/kick"), vec![FieldType::Int]));
        assert_eq!(events(&p).len(), 2);
        // The same text again changes nothing; an absolute event keeps its kind.
        lane(&mut p, 0, edit("/kick")).unwrap();
        let absolute = LaneEdit::SetPath {
            lane: 1,
            path: "/c".into(),
        };
        lane(&mut p, 0, absolute).unwrap();
        assert_eq!(events(&p)[1].0, OscAddress::Absolute("/c".into()));
        assert!(lane(&mut p, 0, edit("a b")).is_err());
        let missing = LaneEdit::SetPath {
            lane: 9,
            path: "/q".into(),
        };
        assert!(lane(&mut p, 0, missing).is_err());
    }

    #[test]
    fn set_path_re_points_a_shared_lane_creating_an_event_with_the_old_signature() {
        let mut p = project();
        lane(&mut p, 1, LaneEdit::AddPath("/a".into())).unwrap();
        let shared = clip(&p, 0).events[0].definition.clone();
        let edit = LaneEdit::SetPath {
            lane: 0,
            path: "/z".into(),
        };
        lane(&mut p, 0, edit).unwrap();
        // Slot 1 still plays /a; slot 0's lane and both hits moved to a new /z (i).
        assert_eq!(path(&p, &shared), relative("/a"));
        assert_eq!(clip(&p, 1).lanes[0].event().unwrap().definition, shared);
        assert_eq!(events(&p)[2], (relative("/z"), vec![FieldType::Int]));
        let moved = &clip(&p, 0).lane_refs()[0];
        assert_ne!(*moved, shared);
        assert_eq!(
            clip(&p, 0)
                .events
                .iter()
                .filter(|e| e.definition == *moved)
                .count(),
            2
        );
        // A path an event already has re-points instead of renaming: no event is added.
        let edit = LaneEdit::SetPath {
            lane: 0,
            path: "/a".into(),
        };
        lane(&mut p, 1, LaneEdit::Remove { lane: 0 }).unwrap();
        lane(&mut p, 0, edit).unwrap();
        assert_eq!(clip(&p, 0).lane_refs()[0], shared);
        assert_eq!(events(&p).len(), 3);
    }

    #[test]
    fn re_pointing_conforms_hits_to_the_event_they_now_play() {
        let mut p = project();
        let target = p
            .ensure_event(
                TrackId(1),
                relative("/f"),
                &[FieldType::Float, FieldType::String],
            )
            .unwrap();
        let edit = LaneEdit::SetPath {
            lane: 0,
            path: "/f".into(),
        };
        lane(&mut p, 0, edit).unwrap();
        let hits: Vec<_> = clip(&p, 0)
            .events
            .iter()
            .filter(|e| e.definition == target)
            .map(|e| e.arguments.clone())
            .collect();
        let conformed = vec![OscArgument::Float(1.0), OscArgument::String(String::new())];
        assert_eq!(hits, vec![conformed.clone(), conformed]);
        // Another lane of the clip already plays /f now.
        let err = lane(&mut p, 0, LaneEdit::AddPath("/f".into())).unwrap_err();
        assert!(err.to_string().contains("another lane already plays"));
    }

    #[test]
    fn remove_drops_the_lane_and_its_hits_and_keeps_the_definition() {
        let mut p = project();
        lane(&mut p, 0, LaneEdit::Remove { lane: 0 }).unwrap();
        assert_eq!(clip_lanes(&p), vec![OscAddress::Absolute("/b".into())]);
        assert_eq!(clip(&p, 0).events.len(), 1);
        assert_eq!(events(&p).len(), 2);
        assert!(lane(&mut p, 0, LaneEdit::Remove { lane: 1 }).is_err());
    }

    #[test]
    fn repoint_removes_only_an_unused_unnamed_argument_free_event() {
        for (name, arguments, shared, retained) in [
            ("", false, false, false),
            ("Cue", false, false, true),
            ("", true, false, true),
            ("", false, true, true),
        ] {
            let mut p = project();
            let old = p.ensure_event(TrackId(1), relative("/event"), &[]).unwrap();
            let definition = p
                .track_mut(TrackId(1))
                .unwrap()
                .event_def_mut(&old)
                .unwrap();
            definition.name = name.into();
            if arguments {
                definition
                    .fields
                    .push(FieldDef::new(FieldKey("f1".into()), FieldType::Int, false));
            }
            lane(&mut p, 1, LaneEdit::AddPath("/event".into())).unwrap();
            if shared {
                lane(&mut p, 0, LaneEdit::AddPath("/event".into())).unwrap();
            }
            lane(
                &mut p,
                1,
                LaneEdit::SetPath {
                    lane: 0,
                    path: "/a".into(),
                },
            )
            .unwrap();
            let track = p.track(TrackId(1)).unwrap();
            assert_eq!(
                track.event_def(&old).is_some(),
                retained,
                "{name:?}, arguments={arguments}, shared={shared}"
            );
            assert_eq!(
                clip(&p, 1).lanes[0].event().unwrap().definition,
                clip(&p, 0).events[0].definition
            );
            assert_eq!(track.devices.len(), 1);
        }
    }

    #[test]
    fn rejected_repoint_keeps_the_placeholder_and_project_unchanged() {
        let mut p = project();
        lane(&mut p, 0, LaneEdit::AddPath("/event".into())).unwrap();
        let before = p.clone();
        assert!(
            lane(
                &mut p,
                0,
                LaneEdit::SetPath {
                    lane: 2,
                    path: "/a".into()
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }

    #[test]
    fn unlinked_regions_preserve_points_relink_and_reject_events_or_bad_markers_atomically() {
        let mut p = project();
        let target = ArgumentRef::volume();
        let points = vec![Breakpoint {
            time: BeatTime(9000),
            value: 0.5,
            curve: 0.0,
        }];
        let markers = clip(&p, 0).region();
        let definition = clip(&p, 0).events[0].definition.clone();
        p.clip_mut(TrackId(1), 0).unwrap().lanes = vec![
            Lane::Events(EventLane {
                definition: definition.clone(),
            }),
            Lane::Envelope(AutomationEnvelope {
                target: target.clone(),
                region: None,
                points: points.clone(),
            }),
        ];
        lane(
            &mut p,
            0,
            LaneEdit::SetRegion {
                lane: LaneKey::Envelope(target.clone()),
                region: Some(markers),
            },
        )
        .unwrap();
        let envelope = clip(&p, 0).lanes[1].envelope().unwrap();
        assert_eq!(envelope.region, Some(markers));
        assert_eq!(envelope.points, points);
        // A queued identity-based command still targets the envelope after a lane move.
        lane(&mut p, 0, LaneEdit::Move { from: 1..2, to: 0 }).unwrap();
        let mut own = markers;
        own.loop_start = BeatTime(960);
        own.loop_end = BeatTime(1920);
        own.start = BeatTime(2880);
        lane(
            &mut p,
            0,
            LaneEdit::SetRegion {
                lane: LaneKey::Envelope(target.clone()),
                region: Some(own),
            },
        )
        .unwrap();
        assert_eq!(
            clip(&p, 0).lanes[0]
                .envelope()
                .unwrap()
                .region
                .unwrap()
                .start,
            BeatTime(960)
        );
        let before = p.clone();
        own.loop_end = own.loop_start;
        for edit in [
            LaneEdit::SetRegion {
                lane: LaneKey::Envelope(target.clone()),
                region: Some(own),
            },
            LaneEdit::SetRegion {
                lane: LaneKey::Events(definition.clone()),
                region: Some(markers),
            },
            LaneEdit::SetRegion {
                lane: LaneKey::Events(definition.clone()),
                region: None,
            },
            LaneEdit::SetRegion {
                lane: LaneKey::Envelope(ArgumentRef {
                    field: FieldKey("missing".into()),
                    ..target.clone()
                }),
                region: None,
            },
        ] {
            assert!(lane(&mut p, 0, edit).is_err());
            assert_eq!(p, before);
        }
        lane(
            &mut p,
            0,
            LaneEdit::SetRegion {
                lane: LaneKey::Envelope(target.clone()),
                region: None,
            },
        )
        .unwrap();
        let envelope = clip(&p, 0).lanes[0].envelope().unwrap();
        assert_eq!(envelope.region, None);
        assert_eq!(envelope.points, points);
        assert_eq!(clip(&p, 0).region(), markers);
    }

    #[test]
    fn validation_rejects_a_duplicate_lane_or_an_unresolved_ref() {
        let mut p = project();
        let definition = clip(&p, 0).events[0].definition.clone();
        let duplicate = vec![
            crate::model::Lane::Events(EventLane {
                definition: definition.clone(),
            }),
            crate::model::Lane::Events(EventLane {
                definition: definition.clone(),
            }),
        ];
        p.tracks[0].slots[0].clip.as_mut().unwrap().lanes = duplicate;
        assert!(p.validate().is_err());
        let mut p = project();
        p.tracks[0].slots[0].clip.as_mut().unwrap().events[0]
            .definition
            .key = EntryKey("gone".into());
        assert!(p.validate().is_err());
        let mut p = project();
        p.tracks[0].slots[0].clip.as_mut().unwrap().events[0].arguments = vec![];
        assert!(p.validate().is_err());
    }
}
