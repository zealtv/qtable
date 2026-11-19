//! Clip marker field semantics shared by the box and ruler. Playback comes only from snapshots.
use super::musical_time::{parse_duration, parse_position};
use crate::{app::App, drafts::ClipField};
use qtable_engine::{
    edit::ProjectEdit,
    engine::Command,
    model::{BeatTime, Clip, GlobalQuantisation, Lane, TimeSignature, TrackId, region::Region},
};

/// Content remains visible and editable beyond the played range, rounded up to a bar.
pub(crate) fn extent(clip: &Clip, signature: TimeSignature) -> BeatTime {
    let content = clip
        .events
        .iter()
        .map(|e| e.position.ticks())
        .chain(clip.lanes.iter().filter_map(|l| {
            match l {
                Lane::Envelope(e) => e
                    .points
                    .iter()
                    .max_by_key(|p| p.time)
                    .map(|p| p.time.ticks()),
                _ => None,
            }
        }))
        .max()
        .unwrap_or(0);
    let lane_end = clip
        .lanes
        .iter()
        .filter_map(|l| l.envelope()?.region)
        .map(|r| r.end.ticks().max(r.loop_end.ticks()))
        .max()
        .unwrap_or(0);
    let end = clip
        .end
        .ticks()
        .max(clip.loop_end.ticks())
        .max(content)
        .max(lane_end);
    let bar = signature.bar_length().ticks();
    BeatTime(end.div_ceil(bar).saturating_mul(bar).max(bar))
}

pub(crate) fn change(mut r: Region, field: ClipField, at: BeatTime) -> Result<Region, String> {
    match field {
        ClipField::Start => r.start = at,
        ClipField::End => r.end = at,
        ClipField::Position => {
            let length = r.loop_end.ticks() - r.loop_start.ticks();
            r.loop_start = at;
            r.loop_end = BeatTime(
                at.ticks()
                    .checked_add(length)
                    .ok_or("Loop end is too large")?,
            );
        }
        ClipField::Length => {
            r.loop_end = BeatTime(
                r.loop_start
                    .ticks()
                    .checked_add(at.ticks())
                    .ok_or("Loop end is too large")?,
            )
        }
        _ => return Err("Not a clip marker".into()),
    }
    Ok(r)
}

pub(crate) fn text(app: &mut App, track: TrackId, slot: usize, field: ClipField, value: String) {
    let Some(clip) = app.project().clip(track, slot) else {
        return;
    };
    let signature = app.project().time_signature;
    let parsed = if field == ClipField::Length {
        parse_duration(&value, signature)
    } else {
        parse_position(&value, signature)
    };
    let result = parsed.and_then(|at| change(clip.region(), field, at));
    let key = super::field_key(track, slot, field);
    match result {
        Ok(markers) => crate::inspector_update::execute_inspector_edit(
            app,
            key,
            ProjectEdit::SetClipMarkers {
                track,
                slot,
                markers,
            },
        ),
        Err(error) => {
            app.drafts.errors.insert(key, error);
        }
    }
}

pub(crate) fn set(app: &mut App, track: TrackId, slot: usize, field: ClipField) {
    let Some(clip) = app.project().clip(track, slot) else {
        return;
    };
    let Some(position) = app
        .snapshot
        .clip_positions
        .iter()
        .find(|p| p.track == track && p.clip == clip.id)
        .map(|p| p.position)
    else {
        return;
    };
    let step = match app.project().global_quantisation {
        GlobalQuantisation::None => 1,
        GlobalQuantisation::Value(q) => q
            .duration(app.project().time_signature)
            .map_or(1, |t| t.ticks()),
    };
    let at = BeatTime(position.ticks().saturating_add(step / 2) / step * step);
    // Length's Set places the loop end at the playhead, rather than treating the
    // absolute content position as a duration.
    let value = if field == ClipField::Length {
        BeatTime(at.ticks().saturating_sub(clip.loop_start.ticks()))
    } else {
        at
    };
    if let Ok(markers) = change(clip.region(), field, value) {
        crate::inspector_update::execute_inspector_edit(
            app,
            super::field_key(track, slot, field),
            ProjectEdit::SetClipMarkers {
                track,
                slot,
                markers,
            },
        );
    }
}

pub(crate) fn toggle(app: &mut App, track: TrackId, slot: usize) {
    let Some(mut markers) = app.project().clip(track, slot).map(Clip::region) else {
        return;
    };
    markers.looping = !markers.looping;
    crate::edit_actions::execute_edit(
        app,
        Command::Edit(ProjectEdit::SetClipMarkers {
            track,
            slot,
            markers,
        }),
    );
}

pub(crate) fn nudge(app: &mut App, right: bool) -> bool {
    let Some((track, slot, _)) = app.clip_view.editor.owner else {
        return false;
    };
    let Some(mut r) = app.project().clip(track, slot).map(Clip::region) else {
        return false;
    };
    let step = app.clip_view.editor.grid.ticks();
    let delta = if right {
        step.min(u64::MAX - r.end.ticks())
    } else {
        step.min(r.start.ticks())
    };
    let shift = |t: BeatTime| {
        BeatTime(if right {
            t.ticks() + delta
        } else {
            t.ticks() - delta
        })
    };
    r.start = shift(r.start);
    r.end = shift(r.end);
    crate::edit_actions::execute_edit(
        app,
        Command::Edit(ProjectEdit::SetClipMarkers {
            track,
            slot,
            markers: r,
        }),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::model::{ClipId, LIVE_CLIP_PALETTE};
    #[test]
    fn axis_keeps_end_loop_and_content_in_content_coordinates() {
        let mut clip = Clip::new(ClipId(1), "intro", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
        clip.start = BeatTime(480);
        clip.end = BeatTime(6000);
        clip.loop_start = BeatTime(960);
        clip.loop_end = BeatTime(2880);
        assert_eq!(extent(&clip, TimeSignature::default()), BeatTime(7680));
        clip.looping = false;
        assert_eq!(extent(&clip, TimeSignature::default()), BeatTime(7680));
        let mut track = qtable_engine::model::Track::new(TrackId(1), "t", 0);
        clip.events.push(track.hit(
            BeatTime(9000),
            qtable_engine::model::OscAddress::Relative("/hit".into()),
            vec![],
            || qtable_engine::model::DeviceId(1),
        ));
        assert_eq!(extent(&clip, TimeSignature::default()), BeatTime(11520));
    }
    #[test]
    fn moving_position_keeps_loop_length_and_length_is_independent_of_loop_switch() {
        let r = Region {
            start: BeatTime(480),
            end: BeatTime(5760),
            loop_start: BeatTime(960),
            loop_end: BeatTime(2880),
            looping: false,
        };
        let moved = change(r, ClipField::Position, BeatTime(1920)).unwrap();
        assert_eq!(moved.loop_end, BeatTime(3840));
        assert_eq!((moved.start, moved.end), (r.start, r.end));
        let longer = change(r, ClipField::Length, BeatTime(3840)).unwrap();
        assert_eq!(longer.loop_end, BeatTime(4800));
        assert_eq!(longer.end, r.end);
    }
}
