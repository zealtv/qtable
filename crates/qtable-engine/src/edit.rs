//! Pure edits to the project grid. Callers apply these to a clone and submit
//! the validated result through the engine command layer.
use crate::model::{
    BeatTime, Clip, ClipFollowActions, Colour, Event, LaunchQuantisation, ModelError, Project,
    Route, SceneId, Slot, Track, TrackId,
};

mod clips;
mod envelopes;
pub use envelopes::EnvelopeEdit;
mod devices;
mod duplicate;
mod groups;
pub use groups::grouping_plan;
mod label;
mod lane_list;
mod lanes;
mod mappings;
mod membership;
mod placement;
pub use mappings::MappingEdit;
pub(crate) use mappings::{deleted_mappings, remove_deleted_controls};
pub use placement::{PlacementPlan, placement_plan};
mod mixer;
mod paste_tracks;
mod rehome;
mod replace_range;
mod report;
mod routes;
mod scenes;
mod target;
use groups::get_group;

pub use lanes::{LaneContent, LaneEdit, LaneKey, PointReplacement};
pub use rehome::Carried;
pub use report::EditReport;
pub use target::TargetEdit;

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectEdit {
    EditMapping(MappingEdit),
    EditEnvelope {
        track: TrackId,
        slot: usize,
        edit: EnvelopeEdit,
    },
    /// Insert at the requested position, clamped to the kind's category run.
    AddDevice {
        track: TrackId,
        kind: crate::model::NewDevice,
        before: Option<crate::model::DeviceId>,
    },
    PasteDevices {
        track: TrackId,
        devices: Vec<crate::model::Device>,
        before: Option<crate::model::DeviceId>,
    },
    /// Copies in chain order, directly after the last selected device.
    DuplicateDevices {
        devices: Vec<crate::model::DeviceId>,
    },
    MoveDevice {
        device: crate::model::DeviceId,
        before: Option<crate::model::DeviceId>,
    },
    /// Also removes the lanes and hits that play these devices on their track.
    RemoveDevices {
        devices: Vec<crate::model::DeviceId>,
    },
    RenameDevice {
        device: crate::model::DeviceId,
        name: String,
    },
    SetDeviceExpanded {
        device: crate::model::DeviceId,
        expanded: bool,
    },
    SetDevicesActive {
        devices: Vec<crate::model::DeviceId>,
        active: bool,
    },
    EditTarget {
        device: crate::model::DeviceId,
        edit: TargetEdit,
    },
    SetParameter {
        target: crate::model::ParameterRef,
        values: Vec<(crate::model::FieldKey, f64)>,
    },
    /// Set stored values across parameters atomically. Each field may appear only once.
    SetParameters(
        Vec<(
            crate::model::ParameterRef,
            Vec<(crate::model::FieldKey, f64)>,
        )>,
    ),
    SetParameterOutput {
        target: crate::model::ParameterRef,
        output: crate::model::ParameterOutput,
    },
    SetTrackPath {
        track: TrackId,
        path: String,
    },
    AddOutput {
        name: String,
        host: String,
        port: u16,
    },
    /// Set an output or Main's optional destination component, including its stage settings.
    SetDestination {
        output: TrackId,
        destination: Option<crate::model::Output>,
    },
    /// Set the delay of any track kind (output latency uses the same model field).
    SetTrackDelay {
        track: TrackId,
        delay_ms: f64,
    },
    /// Replace the output stage of an output track or Main, which must have a destination.
    SetOutputStage {
        output: TrackId,
        stage: crate::model::OutputStage,
    },
    /// Set every listed route atomically, rejecting the whole edit if any entry is invalid.
    SetTrackRoutes {
        routes: Vec<(TrackId, Route)>,
    },
    /// Untick a destination only when every listed track explicitly routes to it; otherwise
    /// tick it on all tracks, replacing Group inheritance. Keep Main, then project output order.
    ToggleOutputInRoute {
        tracks: Vec<TrackId>,
        output: TrackId,
    },
    AddTrack {
        name: String,
    },
    /// Remove columns and group members together, or reject the whole edit.
    RemoveTracks {
        tracks: Vec<TrackId>,
    },
    RenameTrack {
        track: TrackId,
        name: String,
    },
    RecolourTrack {
        track: TrackId,
        colour: Colour,
    },
    MoveTrack {
        from: usize,
        to: usize,
    },
    /// Drag a track: before `before` (last when None), a member of `group` or of none.
    PlaceTrack {
        track: TrackId,
        before: Option<TrackId>,
        group: Option<TrackId>,
    },
    /// Move selected subtree roots together in project order, atomically.
    PlaceTracks {
        tracks: Vec<TrackId>,
        before: Option<TrackId>,
        group: Option<TrackId>,
    },
    /// Paste copies of `tracks` (one family, a group followed by its members) with fresh
    /// ids, placed as `PlaceTrack` places a track.
    PasteTracks {
        tracks: Vec<Track>,
        before: Option<TrackId>,
        group: Option<TrackId>,
    },
    CreateGroup {
        tracks: Vec<TrackId>,
    },
    Ungroup {
        group: TrackId,
    },
    /// Ungroup selected headers atomically, including overlapping ancestors.
    UngroupTracks {
        groups: Vec<TrackId>,
    },
    SetGroupFolded {
        group: TrackId,
        folded: bool,
    },
    AddScene {
        at: usize,
        name: String,
    },
    RemoveScene {
        scene: SceneId,
    },
    /// Remove a selection atomically, including its slots and remote mappings.
    RemoveScenes {
        scenes: Vec<SceneId>,
    },
    RenameScene {
        scene: SceneId,
        name: String,
    },
    SetSceneFollowActions {
        scene: SceneId,
        follow_actions: crate::model::SceneFollowActions,
    },
    /// Move stable scene IDs in project order to a boundary in the original list.
    MoveScenes {
        scenes: Vec<SceneId>,
        boundary: usize,
    },
    MoveScene {
        from: usize,
        to: usize,
    },
    CreateClip {
        track: TrackId,
        slot: usize,
        length: crate::model::BeatTime,
    },
    DeleteClip {
        track: TrackId,
        slot: usize,
    },
    /// Copies directly below, inserting a scene if that slot is occupied or missing.
    DuplicateClip {
        track: TrackId,
        slot: usize,
    },
    /// Puts a copy of `clip` (fresh id) into the slot, replacing any clip already there.
    /// Refs that don't resolve on `track` are adopted from `carried` (`Carried::from_clip`).
    PasteClip {
        track: TrackId,
        slot: usize,
        clip: Box<Clip>,
        carried: Vec<Carried>,
    },
    /// Moves to an existing slot, replacing its clip and retaining both stop flags. A clip
    /// moved to another track brings the definitions it plays.
    MoveClip {
        from: (TrackId, usize),
        to: (TrackId, usize),
    },
    RecolourClip {
        track: TrackId,
        slot: usize,
        colour: Colour,
    },
    RenameClip {
        track: TrackId,
        slot: usize,
        name: String,
    },
    SetClipMarkers {
        track: TrackId,
        slot: usize,
        markers: crate::model::region::Region,
    },
    SetClipLength {
        track: TrackId,
        slot: usize,
        length: BeatTime,
    },
    SetClipLooping {
        track: TrackId,
        slot: usize,
        looping: bool,
    },
    SetClipLegato {
        track: TrackId,
        slot: usize,
        legato: bool,
    },
    SetClipQuantisation {
        track: TrackId,
        slot: usize,
        quantisation: LaunchQuantisation,
    },
    SetClipFollowActions {
        track: TrackId,
        slot: usize,
        follow_actions: ClipFollowActions,
    },
    /// The hit must play an event on the clip's track, with its signature; a hit pasted
    /// from another track carries its definition to adopt.
    AddEvent {
        track: TrackId,
        slot: usize,
        event: Event,
        carried: Option<Carried>,
    },
    RemoveEvent {
        track: TrackId,
        slot: usize,
        event: usize,
    },
    UpdateEvent {
        track: TrackId,
        slot: usize,
        event: usize,
        value: Event,
    },
    /// Edit event paths or move, remove and paste mixed clip lanes (`LaneEdit`).
    EditLane {
        track: TrackId,
        slot: usize,
        edit: LaneEdit,
    },
    SetSlotStopButton {
        track: TrackId,
        slot: usize,
        enabled: bool,
    },
    /// Track Activator on or off for every listed strip with switches, including outputs.
    SetActive {
        strips: Vec<TrackId>,
        active: bool,
    },
    /// Solo tracks routed to this output, preserving routed member solos.
    SetOutputSolo {
        output: TrackId,
        solo: bool,
        exclusive: bool,
    },
    /// Solo on or off for every listed strip; `exclusive` clears other solos.
    SetSolo {
        strips: Vec<TrackId>,
        solo: bool,
        exclusive: bool,
    },
}

/// Apply `edit` to the project, or leave it untouched and say why not. The report names what
/// went or converted with it.
pub fn apply(project: &mut Project, edit: ProjectEdit) -> Result<EditReport, ModelError> {
    // Stored-value edits cannot change structure. The setter checks the entire
    // batch against its field specs before writing anything, so it is atomic
    // without a candidate clone or whole-project validation.
    let edit = match edit {
        ProjectEdit::SetParameter { target, values } => {
            project.set_parameter_values(&target, values)?;
            return Ok(EditReport::default());
        }
        ProjectEdit::SetParameters(parameters) => {
            project.set_parameters_values(parameters)?;
            return Ok(EditReport::default());
        }
        edit => edit,
    };
    let mut candidate = project.clone();
    let conform = matches!(
        edit,
        ProjectEdit::EditTarget { .. } | ProjectEdit::RemoveDevices { .. }
    );
    let mut report = apply_inner(&mut candidate, edit)?;
    if conform {
        envelopes::conform(project, &mut candidate, &mut report);
        // Event and envelope cleanup can affect the same clip; count it once.
        report.removed_in_clips = project
            .tracks
            .iter()
            .flat_map(|t| {
                t.slots
                    .iter()
                    .filter_map(|s| s.clip.as_ref())
                    .map(move |c| (t.id, c))
            })
            .filter(|(id, old)| {
                candidate
                    .track(*id)
                    .and_then(|t| {
                        t.slots
                            .iter()
                            .filter_map(|s| s.clip.as_ref())
                            .find(|c| c.id == old.id)
                    })
                    .is_some_and(|new| {
                        new.lanes.len() < old.lanes.len()
                            || new.lane_refs().len() < old.lane_refs().len()
                            || new.events.len() < old.events.len()
                    })
            })
            .count();
    }
    mappings::remove_deleted_controls(project, &mut candidate, &mut report);
    candidate.validate_stored()?;
    *project = candidate;
    Ok(report)
}

fn apply_inner(project: &mut Project, edit: ProjectEdit) -> Result<EditReport, ModelError> {
    let err = |s: &str| ModelError(s.to_owned());
    let mut report = EditReport::default();
    match edit {
        ProjectEdit::EditMapping(edit) => report = mappings::apply(project, edit)?,
        ProjectEdit::EditEnvelope { track, slot, edit } => {
            report = envelopes::apply(project, track, slot, edit)?
        }
        ProjectEdit::AddDevice {
            track,
            kind,
            before,
        } => {
            let device = kind.build(project.next_device_id());
            devices::paste(project, track, vec![device], before)?;
        }
        ProjectEdit::PasteDevices {
            track,
            devices,
            before,
        } => {
            devices::paste(project, track, devices, before)?;
        }
        ProjectEdit::DuplicateDevices { devices } => devices::duplicate(project, devices)?,
        ProjectEdit::MoveDevice { device, before } => {
            devices::move_device(project, device, before)?
        }
        ProjectEdit::RemoveDevices { devices } => report = devices::remove(project, devices)?,
        ProjectEdit::RenameDevice { device, name } => {
            project
                .device_mut(device)
                .ok_or_else(|| err("unknown device"))?
                .name = name;
        }
        ProjectEdit::SetDeviceExpanded { device, expanded } => {
            project
                .device_mut(device)
                .ok_or_else(|| err("unknown device"))?
                .expanded = expanded;
        }
        ProjectEdit::SetDevicesActive { devices, active } => {
            devices::set_active(project, devices, active)?;
        }
        ProjectEdit::EditTarget { device, edit } => report = target::apply(project, device, edit)?,
        ProjectEdit::SetParameter { target, values } => {
            project.set_parameter_values(&target, values)?;
        }
        ProjectEdit::SetParameters(parameters) => project.set_parameters_values(parameters)?,
        ProjectEdit::SetParameterOutput { target, mut output } => {
            let previous = project
                .parameter(&target)
                .ok_or_else(|| err("parameter not found"))?;
            if output.stream_rate != previous.output.stream_rate {
                crate::model::STREAM_RATE
                    .check(output.stream_rate)
                    .map_err(ModelError)?;
            }
            output.address = crate::model::lead_with_slash(output.address);
            crate::model::validate_address(&output.address)?;
            project
                .parameter_mut(&target)
                .ok_or_else(|| err("parameter not found"))?
                .output = output;
        }
        ProjectEdit::SetTrackPath { track, path } => {
            project
                .track_mut(track)
                .ok_or_else(|| err("track not found"))?
                .path = crate::model::Path::new(crate::model::lead_with_slash(path))?;
        }
        ProjectEdit::AddOutput { name, host, port } => {
            routes::add_output(project, name, host, port)?
        }
        ProjectEdit::SetDestination {
            output,
            destination,
        } => {
            routes::output_mut(project, output)?.output = destination;
        }
        ProjectEdit::SetTrackDelay { track, delay_ms } => {
            get_track(project, track)?.delay_ms = delay_ms;
        }
        ProjectEdit::SetOutputStage { output, stage } => {
            routes::set_output_stage(project, output, stage)?
        }
        ProjectEdit::SetTrackRoutes { routes } => routes::set_routes(project, routes)?,
        ProjectEdit::ToggleOutputInRoute { tracks, output } => {
            routes::toggle_output_in_route(project, tracks, output)?
        }
        ProjectEdit::AddTrack { name } => {
            let id = TrackId(project.next_strip_id());
            let mut track = Track::new(id, name, project.scenes.len());
            track.colour = project.take_next_colour();
            track.route = Route::single(project.main.id);
            project.tracks.push(track);
        }
        ProjectEdit::RemoveTracks { tracks } => groups::remove_tracks(project, tracks)?,
        ProjectEdit::RenameTrack { track, name } => routes::rename_track(project, track, name)?,
        ProjectEdit::RecolourTrack { track, colour } => {
            let found = get_track(project, track)?;
            if !found.capabilities().recolourable {
                return Err(ModelError("this track cannot be recoloured".into()));
            }
            found.colour = colour;
        }
        ProjectEdit::PlaceTrack {
            track,
            before,
            group,
        } => membership::place_track(project, track, before, group)?,
        ProjectEdit::PlaceTracks {
            tracks,
            before,
            group,
        } => membership::place_tracks(project, &tracks, before, group)?,
        ProjectEdit::PasteTracks {
            tracks,
            before,
            group,
        } => paste_tracks::apply(project, tracks, before, group)?,
        ProjectEdit::MoveTrack { from, to } => {
            let mut candidate = project.clone();
            groups::move_track(&mut candidate, from, to)?;
            candidate.validate_stored()?;
            *project = candidate;
        }
        ProjectEdit::CreateGroup { tracks } => groups::create_group(project, tracks)?,
        ProjectEdit::Ungroup { group } => groups::ungroup(project, group)?,
        ProjectEdit::UngroupTracks { groups } => groups::ungroup_tracks(project, groups)?,
        ProjectEdit::SetGroupFolded { group, folded } => {
            get_group(project, group)?.fold_state = folded
        }
        ProjectEdit::AddScene { at, name } => scenes::add(project, at, name)?,
        ProjectEdit::RemoveScene { scene } => scenes::remove(project, &[scene])?,
        ProjectEdit::RemoveScenes { scenes } => scenes::remove(project, &scenes)?,
        ProjectEdit::RenameScene { scene, name } => scenes::rename(project, scene, name)?,
        ProjectEdit::SetSceneFollowActions {
            scene,
            follow_actions,
        } => scenes::set_follow_actions(project, scene, follow_actions)?,
        ProjectEdit::MoveScenes { scenes, boundary } => {
            scenes::move_scenes(project, &scenes, boundary)?
        }
        ProjectEdit::MoveScene { from, to } => scenes::move_scene(project, from, to)?,
        ProjectEdit::CreateClip {
            track,
            slot,
            length,
        } => clips::create(project, track, slot, length)?,
        ProjectEdit::DeleteClip { track, slot } => clips::delete(project, track, slot)?,
        ProjectEdit::DuplicateClip { track, slot } => duplicate::apply(project, track, slot)?,
        ProjectEdit::PasteClip {
            track,
            slot,
            clip,
            carried,
        } => report = clips::paste(project, track, slot, clip, carried)?,
        ProjectEdit::MoveClip { from, to } => report = clips::move_clip(project, from, to)?,
        ProjectEdit::RecolourClip {
            track,
            slot,
            colour,
        } => clips::recolour(project, track, slot, colour)?,
        ProjectEdit::RenameClip { track, slot, name } => clips::rename(project, track, slot, name)?,
        ProjectEdit::SetClipMarkers {
            track,
            slot,
            markers,
        } => clips::set_markers(project, track, slot, markers)?,
        ProjectEdit::SetClipLength {
            track,
            slot,
            length,
        } => clips::set_length(project, track, slot, length)?,
        ProjectEdit::SetClipLooping {
            track,
            slot,
            looping,
        } => clips::set_looping(project, track, slot, looping)?,
        ProjectEdit::SetClipLegato {
            track,
            slot,
            legato,
        } => clips::set_legato(project, track, slot, legato)?,
        ProjectEdit::SetClipQuantisation {
            track,
            slot,
            quantisation,
        } => clips::set_quantisation(project, track, slot, quantisation)?,
        ProjectEdit::SetClipFollowActions {
            track,
            slot,
            follow_actions,
        } => clips::set_follow_actions(project, track, slot, follow_actions)?,
        ProjectEdit::AddEvent {
            track,
            slot,
            event,
            carried,
        } => report = clips::add_event(project, track, slot, event, carried)?,
        ProjectEdit::RemoveEvent { track, slot, event } => {
            clips::remove_event(project, track, slot, event)?
        }
        ProjectEdit::UpdateEvent {
            track,
            slot,
            event,
            value,
        } => clips::update_event(project, track, slot, event, value)?,
        ProjectEdit::EditLane { track, slot, edit } => {
            report = lanes::apply(project, track, slot, edit)?
        }
        ProjectEdit::SetSlotStopButton {
            track,
            slot,
            enabled,
        } => clips::set_slot_stop_button(project, track, slot, enabled)?,
        ProjectEdit::SetActive { strips, active } => mixer::set_active(project, &strips, active)?,
        ProjectEdit::SetOutputSolo {
            output,
            solo,
            exclusive,
        } => mixer::set_output_solo(project, output, solo, exclusive)?,
        ProjectEdit::SetSolo {
            strips,
            solo,
            exclusive,
        } => mixer::set_solo(project, &strips, solo, exclusive)?,
    }
    Ok(report)
}

/// Move a contiguous block to its final start index, shared by tracks and lanes.
fn move_range<T>(
    items: &mut Vec<T>,
    from: std::ops::Range<usize>,
    to: usize,
) -> Result<(), ModelError> {
    if from.start >= from.end || from.end > items.len() || to > items.len() - from.len() {
        return Err(ModelError("move index out of range".into()));
    }
    let block: Vec<_> = items.drain(from).collect();
    items.splice(to..to, block);
    Ok(())
}

fn get_track(project: &mut Project, id: TrackId) -> Result<&mut Track, ModelError> {
    project
        .track_mut(id)
        .ok_or_else(|| ModelError("unknown track".into()))
}
fn get_slot(project: &mut Project, id: TrackId, index: usize) -> Result<&mut Slot, ModelError> {
    get_track(project, id)?;
    project
        .clip_slot_mut(id, index)
        .ok_or_else(|| ModelError("slot index out of range".into()))
}
/// Places `clip` in the slot under a fresh clip ID, appending `suffix` to its name.
fn paste_clip(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    mut clip: Clip,
    suffix: &str,
) -> Result<(), ModelError> {
    clip.id = project.next_clip_id();
    clip.name.push_str(suffix);
    if clip.colour.is_unset() {
        clip.colour = get_track(project, track)?.colour;
    }
    get_slot(project, track, slot)?.clip = Some(clip);
    Ok(())
}
#[cfg(test)]
mod tests;

impl ProjectEdit {
    /// Convenience for Mixer Volume and a newly created Target's first field.
    pub fn set_value(target: crate::model::ParameterRef, value: f64) -> Self {
        let key = match target.device {
            crate::model::DeviceRef::Mixer => crate::model::FieldRef::volume(target.track).field,
            crate::model::DeviceRef::Chain(_) => crate::model::FieldKey("f1".into()),
        };
        Self::SetParameter {
            target,
            values: vec![(key, value)],
        }
    }
}
