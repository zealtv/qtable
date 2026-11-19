//! Paste copied columns as new tracks, groups or outputs.
//!
//! The engine knows nothing about clipboards: it takes whole `Track` values and inserts
//! fresh copies where `PlaceTrack` would put a track. The values form a forest in preorder:
//! every copied Group travels with its whole subtree, at any
//! depth. Ids, clip ids, device ids and group links are made new here, level by level; the
//! group join and leave route rules, applied to each subtree root only, come from
//! `membership::enter`.
use super::{ProjectEdit, apply_inner, membership};
use crate::model::{
    Capabilities, DeviceId, EventRef, ModelError, Project, Region, Route, Slot, Topology, Track,
    TrackId,
};
use std::collections::{HashMap, HashSet};

/// Appended to the name of each pasted column (not to a copied group's members, nor to clips).
/// Manager's default for the build, 2026-10-14: unchanged, as Live does by recollection,
/// pending the maintainer's answer. Set to `" copy"` for qtable's clip convention.
pub(super) const COPY_SUFFIX: &str = "";

fn err(message: &str) -> ModelError {
    ModelError(message.to_owned())
}

pub(super) fn apply(
    project: &mut Project,
    tracks: Vec<Track>,
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<(), ModelError> {
    let first = tracks.first().ok_or_else(|| err("nothing to paste"))?;
    let region = first.capabilities().region;
    if !tracks.iter().all(|t| {
        let c: Capabilities = t.capabilities();
        c.copyable && c.region == region
    }) {
        return Err(err("only tracks of one family can be pasted together"));
    }
    match region {
        Region::Outputs => paste_outputs(project, tracks, before, group),
        Region::Tracks => paste_clip_tracks(project, tracks, before, group),
        Region::Main => Err(err("Main cannot be copied")),
    }
}

fn paste_outputs(
    project: &mut Project,
    outputs: Vec<Track>,
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<(), ModelError> {
    if group.is_some() {
        return Err(err("an output joins no group"));
    }
    let to = match before {
        Some(id) => project
            .outputs
            .iter()
            .position(|o| o.id == id)
            .ok_or_else(|| err("an output is placed among outputs"))?,
        None => project.outputs.len(),
    };
    let mut next_device = project.next_device_id().0;
    let copies: Vec<Track> = outputs
        .into_iter()
        .zip(project.next_strip_id()..)
        .map(|(mut output, id)| {
            output.id = TrackId(id);
            output.name.push_str(COPY_SUFFIX);
            renew_devices(&mut output, &mut next_device);
            output
        })
        .collect();
    project.outputs.splice(to..to, copies);
    Ok(())
}

fn paste_clip_tracks(
    project: &mut Project,
    tracks: Vec<Track>,
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<(), ModelError> {
    for clip in tracks
        .iter()
        .flat_map(|t| &t.slots)
        .filter_map(|s| s.clip.as_ref())
    {
        let actions = &clip.follow_actions.actions;
        if [actions.action_a, actions.action_b].iter().any(|action| {
            matches!(action, crate::model::FollowAction::Jump(id) if !project.scenes.iter().any(|scene| scene.id == *id))
        }) {
            return Err(err("copied tracks jump to a scene that no longer exists"));
        }
    }
    let mut subtrees = into_subtrees(tracks)?;
    if let Some(id) = before
        && !project.tracks.iter().any(|t| t.id == id)
    {
        return Err(err("unknown track"));
    }
    if let Some(id) = group
        && !project.group_tracks().any(|g| g.id == id)
    {
        return Err(err("unknown group"));
    }
    fit_slots(project, subtrees.iter_mut().flatten())?;
    let destinations: Vec<TrackId> = project.destinations().map(|d| d.id).collect();
    let mut next = project.next_strip_id();
    let mut next_clip = project.next_clip_id().0;
    let mut next_device = project.next_device_id().0;
    let mut ids = Ids {
        next: &mut next,
        next_clip: &mut next_clip,
        next_device: &mut next_device,
    };
    for subtree in &mut subtrees {
        renew_subtree(subtree, &mut ids, &destinations);
    }
    membership::enter(project, subtrees, before, group)
}

/// Split the clipboard into whole subtrees, each a root followed by exactly its descendants
/// in preorder. A track whose group is not in the clipboard counts as a root, whatever it
/// named before (a member copied alone becomes a plain track). Everything else must already
/// be a valid forest: the one topology check rejects a parent after its child, a subtree
/// that continues after it closed, a non-Group parent and a repeated id, and a copied Group
/// with no copied child is refused as the nonempty-group policy would.
fn into_subtrees(tracks: Vec<Track>) -> Result<Vec<Vec<Track>>, ModelError> {
    let copied: HashSet<TrackId> = tracks.iter().map(|t| t.id).collect();
    let mut standalone = tracks.clone();
    for track in &mut standalone {
        if track.group.is_some_and(|g| !copied.contains(&g)) {
            track.group = None;
        }
    }
    let topology = Topology::build(&standalone)
        .map_err(|e| err(&format!("copied tracks are not whole groups: {e}")))?;
    if topology.childless_groups().next().is_some() {
        return Err(err("a copied group must include its members"));
    }
    let ids: Vec<TrackId> = tracks.iter().map(|t| t.id).collect();
    let mut tracks = tracks.into_iter();
    let mut subtrees = Vec::new();
    for root in topology.subtree_roots(&ids)? {
        let len = topology.span(root).map_or(0, |span| span.len());
        subtrees.push(tracks.by_ref().take(len).collect());
    }
    Ok(subtrees)
}

/// Fresh ids for every track in a copied subtree, remapping each copied child's group to
/// its parent's new id once. The root keeps its old group so `membership::enter` can apply
/// the join and leave route rules; descendants keep their routes, `Route::Group` included,
/// because they enter the copy of their own parent.
///
// Not yet decided (2026-11-06): a copy carries no mappings. Fresh ids have none, and
// no mapping is cloned for any level, following the existing policy and the design's
// recommendation (nested-groups-design section 4).
fn renew_subtree(subtree: &mut [Track], ids: &mut Ids, destinations: &[TrackId]) {
    let mut renewed: HashMap<TrackId, TrackId> = HashMap::new();
    for (at, track) in subtree.iter_mut().enumerate() {
        let old = track.id;
        renew(track, ids, destinations, at == 0);
        if at > 0 {
            track.group = track.group.and_then(|g| renewed.get(&g).copied());
        }
        renewed.insert(old, track.id);
    }
}

/// The next free track, clip and device ids while pasting.
struct Ids<'a> {
    next: &'a mut u64,
    next_clip: &'a mut u64,
    next_device: &'a mut u64,
}

/// Fresh ids for the track, its clips and its devices, with every lane and hit following its
/// device; routes keep only outputs that still exist. `suffix` marks a top-level pasted column.
fn renew(track: &mut Track, ids: &mut Ids, destinations: &[TrackId], suffix: bool) {
    track.id = TrackId(*ids.next);
    *ids.next += 1;
    if suffix {
        track.name.push_str(COPY_SUFFIX);
    }
    if let Route::Outputs(ids) = &mut track.route {
        ids.retain(|id| destinations.contains(id));
    }
    renew_devices(track, ids.next_device);
    for clip in track.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
        clip.id = crate::model::ClipId(*ids.next_clip);
        *ids.next_clip += 1;
    }
}

fn renew_devices(track: &mut Track, next_device: &mut u64) {
    let mut devices = Vec::new();
    for device in &mut track.devices {
        devices.push((device.id, DeviceId(*next_device)));
        device.id = DeviceId(*next_device);
        *next_device += 1;
    }
    let renew_ref = |reference: &mut EventRef| {
        if let Some((_, new)) = devices.iter().find(|(old, _)| *old == reference.device) {
            reference.device = *new;
        }
    };
    for clip in track.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
        clip.lanes.iter_mut().for_each(|lane| match lane {
            crate::model::Lane::Events(l) => renew_ref(&mut l.definition),
            crate::model::Lane::Envelope(e) => {
                if let crate::model::DeviceRef::Chain(id) = &mut e.target.device
                    && let Some((_, new)) = devices.iter().find(|(old, _)| old == id)
                {
                    *id = *new;
                }
            }
        });
        clip.events
            .iter_mut()
            .for_each(|e| renew_ref(&mut e.definition));
    }
}

/// Make every copied track's slots as long as the project's scene list. Scenes added since
/// the copy pad with empty slots. Scenes removed since drop trailing slots; a clip that would
/// fall off the bottom gets scenes appended for it, as `DuplicateClip` does.
fn fit_slots<'a>(
    project: &mut Project,
    tracks: impl Iterator<Item = &'a mut Track>,
) -> Result<(), ModelError> {
    let mut tracks: Vec<_> = tracks.collect();
    let needed = tracks
        .iter()
        .flat_map(|t| t.slots.iter().rposition(|s| s.clip.is_some()))
        .map(|last| last + 1)
        .max()
        .unwrap_or(0);
    while project.scenes.len() < needed {
        apply_inner(
            project,
            ProjectEdit::AddScene {
                at: project.scenes.len(),
                name: format!("Scene {}", project.scenes.len() + 1),
            },
        )?;
    }
    let count = project.scenes.len();
    for track in &mut tracks {
        track.slots.resize_with(count, Slot::default);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::COPY_SUFFIX;
    use crate::edit::{ProjectEdit, apply};
    use crate::model::{
        BeatTime, Clip, ClipId, FollowAction, LIVE_CLIP_PALETTE, ModelError, OscAddress,
        OscArgument, Project, Route, SceneId, Track, TrackId,
    };

    /// Three scenes; tracks 1–4 with track 2 and 3 in group 101 (sending to output 201);
    /// outputs 201 and 202. Track 1 holds clips 1 (scene 0) and 2 (scene 2).
    fn project() -> Project {
        let mut p = Project {
            outputs: vec![
                Track::new_output(TrackId(201), "A", "127.0.0.1", 9000),
                Track::new_output(TrackId(202), "B", "127.0.0.1", 9001),
            ],
            ..Project::default()
        };
        p.tracks = (1..=4)
            .map(|i| Track::new(TrackId(i), format!("T{i}"), 0))
            .collect();
        for at in 0..3 {
            apply(
                &mut p,
                ProjectEdit::AddScene {
                    at,
                    name: format!("S{at}"),
                },
            )
            .unwrap();
        }
        let mut group = Track::new_group(TrackId(101), "G", 3);
        group.colour = LIVE_CLIP_PALETTE[1];
        group.route = Route::single(TrackId(201));
        p.insert_group(group, &[TrackId(2), TrackId(3)]);
        p[TrackId(1)].route = Route::single(TrackId(202));
        p[TrackId(1)].delay_ms = 12.5;
        p[TrackId(1)].colour = LIVE_CLIP_PALETTE[5];
        p[TrackId(1)].path = crate::model::Path::new("/p").unwrap();
        p[TrackId(1)].mixer.solo = true;
        p[TrackId(2)].route = Route::Group;
        p[TrackId(3)].route = Route::single(TrackId(202));
        for (slot, id) in [(0, 1), (2, 2)] {
            let mut clip = Clip::new(
                ClipId(id),
                format!("c{id}"),
                BeatTime(960),
                LIVE_CLIP_PALETTE[2],
            )
            .unwrap();
            let x = OscAddress::Relative("/x".into());
            let arguments = vec![OscArgument::Int(id as i32)];
            clip.events
                .push(p.hit(TrackId(1), BeatTime(0), x, arguments).unwrap());
            p[TrackId(1)].slots[slot].clip = Some(clip);
        }
        p.validate().unwrap();
        p
    }

    fn paste(
        p: &mut Project,
        tracks: Vec<Track>,
        before: Option<u64>,
        group: Option<u64>,
    ) -> Result<crate::edit::EditReport, ModelError> {
        apply(
            p,
            ProjectEdit::PasteTracks {
                tracks,
                before: before.map(TrackId),
                group: group.map(TrackId),
            },
        )
    }

    fn order(p: &Project) -> Vec<u64> {
        p.tracks.iter().map(|t| t.id.0).collect()
    }

    /// The copy of the copied track, found by being the only id the project did not have.
    fn fresh(before: &Project, after: &Project) -> Vec<Track> {
        after
            .tracks
            .iter()
            .filter(|t| before.track(t.id).is_none())
            .cloned()
            .collect()
    }

    #[test]
    fn a_track_pastes_before_a_top_level_track_with_fresh_ids_and_the_same_settings() {
        let before = project();
        let mut p = before.clone();
        let source = p[TrackId(1)].clone();
        paste(&mut p, vec![source.clone()], Some(4), None).unwrap();
        let [copy] = fresh(&before, &p).try_into().unwrap();
        assert_eq!(order(&p), [1, 101, 2, 3, copy.id.0, 4]);
        assert!(copy.id.0 > 202);
        assert_eq!(copy.name, format!("T1{COPY_SUFFIX}"));
        assert_eq!(copy.group, None);
        assert_eq!(copy.route, source.route);
        assert_eq!(copy.colour, source.colour);
        assert_eq!(
            (&copy.path, copy.delay_ms, &copy.mixer, &copy.mixer_device),
            (
                &source.path,
                source.delay_ms,
                &source.mixer,
                &source.mixer_device
            )
        );
        assert_eq!(copy.slots.len(), 3);
        let clips: Vec<_> = copy.slots.iter().map(|s| s.clip.as_ref()).collect();
        assert!(clips[1].is_none());
        for (new, old) in [(0, 0), (2, 2)] {
            let (new, old) = (
                clips[new].unwrap(),
                source.slots[old].clip.as_ref().unwrap(),
            );
            assert_ne!(new.id, old.id);
            // The copy's hits play the same entries on the copy's own Target.
            let mut expected = old.events.clone();
            for event in &mut expected {
                event.definition.device = copy.devices[0].id;
            }
            assert_eq!(
                (&new.events, &new.name, new.colour),
                (&expected, &old.name, old.colour)
            );
        }
        assert_eq!(p[TrackId(1)], source, "the source stays untouched");
        p.validate().unwrap();
    }

    #[test]
    fn pasting_before_a_group_member_joins_that_group_with_a_group_route() {
        let before = project();
        let mut p = before.clone();
        let source = p[TrackId(1)].clone();
        paste(&mut p, vec![source], Some(3), Some(101)).unwrap();
        let [copy] = fresh(&before, &p).try_into().unwrap();
        assert_eq!(order(&p), [1, 101, 2, copy.id.0, 3, 4]);
        assert_eq!(copy.group, Some(TrackId(101)));
        assert_eq!(copy.route, Route::Group);
        // Before the first member from outside: before the group header, at top level.
        let mut p = before.clone();
        let source = p[TrackId(1)].clone();
        paste(&mut p, vec![source], Some(2), None).unwrap();
        let [copy] = fresh(&before, &p).try_into().unwrap();
        assert_eq!(order(&p), [1, copy.id.0, 101, 2, 3, 4]);
        p.validate().unwrap();
    }

    #[test]
    fn a_group_pastes_with_its_members_under_the_new_group_before_a_header() {
        let before = project();
        let mut p = before.clone();
        let mut block = vec![p[TrackId(101)].clone()];
        block.extend(p.members(TrackId(101)).cloned());
        paste(&mut p, block.clone(), Some(4), None).unwrap();
        let new = fresh(&before, &p);
        let [header, a, b] = new.as_slice() else {
            panic!("{new:?}")
        };
        assert_eq!(order(&p), [1, 101, 2, 3, header.id.0, a.id.0, b.id.0, 4]);
        assert!(header.is_group() && header.group.is_none());
        assert_eq!(header.name, format!("G{COPY_SUFFIX}"));
        assert_eq!((a.group, b.group), (Some(header.id), Some(header.id)));
        assert_eq!((a.name.as_str(), b.name.as_str()), ("T2", "T3"));
        assert_eq!(header.route, Route::single(TrackId(201)));
        assert_eq!(
            (&a.route, &b.route),
            (&Route::Group, &Route::single(TrackId(202)))
        );
        // A boundary inside a group's run, from outside that group, splits the run: refused.
        let mut q = before.clone();
        assert!(paste(&mut q, block.clone(), Some(3), None).is_err());
        assert_eq!(q, before);
        // A group joins a group like any root: nested, routed to its new parent.
        paste(&mut q, block, Some(4), Some(101)).unwrap();
        let new = fresh(&before, &q);
        assert_eq!(new[0].group, Some(TrackId(101)));
        assert_eq!(new[0].route, Route::Group);
        assert_eq!(
            (new[1].group, new[2].group),
            (Some(new[0].id), Some(new[0].id))
        );
        q.validate().unwrap();
    }

    #[test]
    fn a_group_member_pasted_at_top_level_goes_to_main() {
        let before = project();
        let mut p = before.clone();
        let member = p[TrackId(2)].clone();
        assert_eq!(member.route, Route::Group);
        paste(&mut p, vec![member.clone()], Some(1), None).unwrap();
        let [copy] = fresh(&before, &p).try_into().unwrap();
        assert_eq!(copy.group, None);
        assert_eq!(copy.route, Route::single(p.main.id));
        // The source group is gone from the project: Main.
        let mut q = before.clone();
        paste(
            &mut q,
            vec![Track {
                group: Some(TrackId(777)),
                ..member
            }],
            Some(1),
            None,
        )
        .unwrap();
        let [copy] = fresh(&before, &q).try_into().unwrap();
        assert_eq!(copy.route, Route::single(q.main.id));
    }

    #[test]
    fn outputs_paste_before_an_output_and_append_without_a_target() {
        let before = project();
        let mut p = before.clone();
        let source = p[TrackId(202)].clone();
        paste(&mut p, vec![source.clone()], Some(201), None).unwrap();
        let ids: Vec<_> = p.outputs.iter().map(|o| o.id.0).collect();
        assert_eq!(ids[1..], [201, 202]);
        let copy = &p.outputs[0];
        assert!(copy.id.0 > 202);
        assert_eq!(copy.name, format!("B{COPY_SUFFIX}"));
        assert_eq!(copy.destination(), source.destination());
        paste(&mut p, vec![source], None, None).unwrap();
        assert_eq!(p.outputs.len(), 4);
        assert_eq!(p.outputs[3].id.0, p.outputs[0].id.0 + 1);
        let mut q = before.clone();
        assert!(paste(&mut q, vec![before.outputs[0].clone()], Some(1), None).is_err());
        assert!(paste(&mut q, vec![before.outputs[0].clone()], None, Some(101)).is_err());
        assert_eq!(q, before);
    }

    #[test]
    fn mixed_families_main_and_broken_group_blocks_are_refused_unchanged() {
        let before = project();
        let mut p = before.clone();
        let (track, output) = (before[TrackId(1)].clone(), before.outputs[0].clone());
        let (header, member) = (before[TrackId(101)].clone(), before[TrackId(2)].clone());
        let other_member = before[TrackId(3)].clone();
        for tracks in [
            vec![],
            vec![track.clone(), output],
            vec![before.main.clone()],
            vec![header.clone()],
            vec![header.clone(), member.clone(), track.clone(), other_member],
            vec![member.clone(), header.clone()],
        ] {
            assert!(
                paste(&mut p, tracks.clone(), None, None).is_err(),
                "{tracks:?}"
            );
            assert_eq!(p, before);
        }
        assert!(paste(&mut p, vec![track], Some(999), None).is_err());
        assert!(paste(&mut p, vec![member], None, Some(999)).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn fewer_scenes_pad_and_more_scenes_drop_empty_slots_or_grow_for_a_clip() {
        let before = project();
        let source = before[TrackId(1)].clone();
        // Scenes added since the copy: pad with empty slots.
        let mut p = before.clone();
        apply(
            &mut p,
            ProjectEdit::AddScene {
                at: 3,
                name: "S3".into(),
            },
        )
        .unwrap();
        paste(&mut p, vec![source.clone()], None, None).unwrap();
        assert!(p.tracks.iter().all(|t| t.slots.len() == 4));
        assert!(p.tracks.last().unwrap().slots[3].clip.is_none());
        // Scenes removed since the copy; the bottom clip fell off: scenes come back for it.
        let mut p = before.clone();
        let scene = p.scenes[2].id;
        apply(&mut p, ProjectEdit::RemoveScene { scene }).unwrap();
        paste(&mut p, vec![source.clone()], None, None).unwrap();
        assert_eq!(p.scenes.len(), 3);
        assert!(p.tracks.iter().all(|t| t.slots.len() == 3));
        assert!(p.tracks.last().unwrap().slots[2].clip.is_some());
        // Trailing empty slots are dropped when no clip needs them.
        let mut short = project();
        for scene in [short.scenes[2].id, short.scenes[1].id] {
            apply(&mut short, ProjectEdit::RemoveScene { scene }).unwrap();
        }
        let mut lean = source.clone();
        lean.slots[2].clip = None;
        let mut q = short.clone();
        paste(&mut q, vec![lean], None, None).unwrap();
        assert!(q.tracks.iter().all(|t| t.slots.len() == 1));
    }

    #[test]
    fn a_jump_to_a_missing_scene_refuses_the_whole_paste() {
        let before = project();
        let mut source = before[TrackId(1)].clone();
        source.slots[0]
            .clip
            .as_mut()
            .unwrap()
            .follow_actions
            .actions
            .enabled = true;
        source.slots[0]
            .clip
            .as_mut()
            .unwrap()
            .follow_actions
            .actions
            .action_a = FollowAction::Jump(SceneId(9999));
        let mut p = before.clone();
        assert_eq!(
            paste(&mut p, vec![source], None, None)
                .unwrap_err()
                .to_string(),
            "copied tracks jump to a scene that no longer exists"
        );
        assert_eq!(p, before);
    }

    #[test]
    fn a_route_to_a_deleted_output_is_filtered() {
        let before = project();
        let mut source = before[TrackId(1)].clone();
        source.route = Route::Outputs(vec![TrackId(201), TrackId(999), before.main.id]);
        let mut p = before.clone();
        paste(&mut p, vec![source], None, None).unwrap();
        assert_eq!(
            p.tracks.last().unwrap().route,
            Route::Outputs(vec![TrackId(201), before.main.id])
        );
    }

    #[test]
    fn several_plain_tracks_paste_in_order_and_each_edit_validates() {
        let before = project();
        let mut p = before.clone();
        let two = vec![p[TrackId(1)].clone(), p[TrackId(4)].clone()];
        paste(&mut p, two, Some(4), None).unwrap();
        let new = fresh(&before, &p);
        assert_eq!(order(&p), [1, 101, 2, 3, new[0].id.0, new[1].id.0, 4]);
        assert_eq!(new[0].name, format!("T1{COPY_SUFFIX}"));
        assert_eq!(new[1].name, format!("T4{COPY_SUFFIX}"));
        p.validate().unwrap();
    }

    #[test]
    fn group_blocks_paste_before_another_group_header_and_at_the_end() {
        let before = project();
        let mut block = vec![before[TrackId(101)].clone()];
        block.extend(before.members(TrackId(101)).cloned());
        for target in [Some(101), None] {
            let mut p = before.clone();
            paste(&mut p, block.clone(), target, None).unwrap();
            let new = fresh(&before, &p);
            let ids: Vec<_> = new.iter().map(|t| t.id.0).collect();
            let expected = if target.is_some() {
                vec![1, ids[0], ids[1], ids[2], 101, 2, 3, 4]
            } else {
                vec![1, 101, 2, 3, 4, ids[0], ids[1], ids[2]]
            };
            assert_eq!(order(&p), expected);
            assert_eq!(new[1].group, Some(new[0].id));
            assert_eq!(new[2].group, Some(new[0].id));
            p.validate().unwrap();
        }
    }

    #[test]
    fn a_plain_track_aimed_inside_before_the_first_member_stays_inside() {
        let before = project();
        let mut p = before.clone();
        paste(&mut p, vec![before[TrackId(1)].clone()], Some(2), Some(101)).unwrap();
        let [copy] = fresh(&before, &p).try_into().unwrap();
        assert_eq!(order(&p), [1, 101, copy.id.0, 2, 3, 4]);
        assert_eq!(copy.group, Some(TrackId(101)));
        assert_eq!(copy.route, Route::Group);
        p.validate().unwrap();
    }
}
