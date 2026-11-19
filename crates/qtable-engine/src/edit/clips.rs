//! Clip and event edits: create, move, recolour and the event lanes/hits they hold.
use super::{Carried, EditReport, get_slot, get_track, paste_clip, rehome};
use crate::model::{
    BeatTime, Clip, ClipFollowActions, Colour, Event, LaunchQuantisation, ModelError, Project,
    TrackId,
};

fn err(message: &str) -> ModelError {
    ModelError(message.to_owned())
}

fn get_clip(project: &mut Project, id: TrackId, index: usize) -> Result<&mut Clip, ModelError> {
    get_slot(project, id, index)?;
    project
        .clip_mut(id, index)
        .ok_or_else(|| err("slot is empty"))
}

pub(super) fn create(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    length: BeatTime,
) -> Result<(), ModelError> {
    let id = project.next_clip_id();
    let colour = get_track(project, track)?.colour;
    let target = get_slot(project, track, slot)?;
    if target.clip.is_some() {
        return Err(err("slot already contains a clip"));
    }
    target.clip = Some(Clip::new(id, "Clip", length, colour)?);
    Ok(())
}

pub(super) fn delete(project: &mut Project, track: TrackId, slot: usize) -> Result<(), ModelError> {
    get_slot(project, track, slot)?.clip = None;
    Ok(())
}

pub(super) fn paste(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    mut clip: Box<Clip>,
    carried: Vec<Carried>,
) -> Result<EditReport, ModelError> {
    let report = rehome::rehome_clip(project, track, &mut clip, &carried)?;
    paste_clip(project, track, slot, *clip, "")?;
    Ok(report)
}

pub(super) fn move_clip(
    project: &mut Project,
    from: (TrackId, usize),
    to: (TrackId, usize),
) -> Result<EditReport, ModelError> {
    let mut clip = get_slot(project, from.0, from.1)?
        .clip
        .clone()
        .ok_or_else(|| err("source slot is empty"))?;
    if from == to {
        return Ok(EditReport::default());
    }
    let carried = Carried::from_clip(get_track(project, from.0)?, &clip);
    let report = rehome::rehome_clip(project, to.0, &mut clip, &carried)?;
    get_slot(project, to.0, to.1)?.clip = Some(clip);
    get_slot(project, from.0, from.1)?.clip = None;
    Ok(report)
}

pub(super) fn recolour(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    colour: Colour,
) -> Result<(), ModelError> {
    get_clip(project, track, slot)?.colour = colour;
    Ok(())
}

pub(super) fn rename(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    name: String,
) -> Result<(), ModelError> {
    get_clip(project, track, slot)?.name = name;
    Ok(())
}

pub(super) fn set_markers(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    markers: crate::model::region::Region,
) -> Result<(), ModelError> {
    let markers = markers.normalised();
    markers.validate()?;
    let clip = get_clip(project, track, slot)?;
    clip.start = markers.start;
    clip.end = markers.end;
    clip.loop_start = markers.loop_start;
    clip.loop_end = markers.loop_end;
    clip.looping = markers.looping;
    Ok(())
}

// Compatibility edits used by the existing clip panel; all marker rules live above.
pub(super) fn set_length(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    length: BeatTime,
) -> Result<(), ModelError> {
    let mut markers = get_clip(project, track, slot)?.region();
    markers.end = BeatTime(
        markers
            .start
            .0
            .checked_add(length.0)
            .ok_or_else(|| err("clip end overflow"))?,
    );
    markers.loop_end = BeatTime(
        markers
            .loop_start
            .0
            .checked_add(length.0)
            .ok_or_else(|| err("loop end overflow"))?,
    );
    set_markers(project, track, slot, markers)
}

pub(super) fn set_looping(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    looping: bool,
) -> Result<(), ModelError> {
    let mut markers = get_clip(project, track, slot)?.region();
    markers.looping = looping;
    set_markers(project, track, slot, markers)
}

pub(super) fn set_legato(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    legato: bool,
) -> Result<(), ModelError> {
    get_clip(project, track, slot)?.legato = legato;
    Ok(())
}

pub(super) fn set_quantisation(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    quantisation: LaunchQuantisation,
) -> Result<(), ModelError> {
    get_clip(project, track, slot)?.quantisation = quantisation;
    Ok(())
}

pub(super) fn set_follow_actions(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    follow_actions: ClipFollowActions,
) -> Result<(), ModelError> {
    get_clip(project, track, slot)?.follow_actions = follow_actions;
    Ok(())
}

pub(super) fn add_event(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    event: Event,
    carried: Option<Carried>,
) -> Result<EditReport, ModelError> {
    let mut clip = get_clip(project, track, slot)?.clone();
    clip.events.push(event);
    let report = rehome::rehome_clip(project, track, &mut clip, carried.as_slice())?;
    *get_clip(project, track, slot)? = clip;
    Ok(report)
}

pub(super) fn remove_event(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    event: usize,
) -> Result<(), ModelError> {
    let events = &mut get_clip(project, track, slot)?.events;
    if event >= events.len() {
        return Err(err("event index out of range"));
    }
    events.remove(event);
    Ok(())
}

pub(super) fn update_event(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    event: usize,
    value: Event,
) -> Result<(), ModelError> {
    let events = &mut get_clip(project, track, slot)?.events;
    *events
        .get_mut(event)
        .ok_or_else(|| err("event index out of range"))? = value;
    Ok(())
}

pub(super) fn set_slot_stop_button(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    enabled: bool,
) -> Result<(), ModelError> {
    get_slot(project, track, slot)?.has_stop_button = enabled;
    Ok(())
}
