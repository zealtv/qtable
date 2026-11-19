//! Output and route edits: the one place outputs are added, changed and removed and routes
//! are set, whether the caller is a track or a group.
use super::get_track;
use crate::model::{LIVE_CLIP_PALETTE, ModelError, OutputStage, Project, Route, Track, TrackId};

fn unknown_output() -> ModelError {
    ModelError("unknown output".into())
}

fn no_route() -> ModelError {
    ModelError("an output has no route".into())
}

pub(super) fn output_mut(project: &mut Project, id: TrackId) -> Result<&mut Track, ModelError> {
    project
        .track_mut(id)
        .filter(|o| o.capabilities().destination)
        .ok_or_else(unknown_output)
}

pub(super) fn add_output(
    project: &mut Project,
    name: String,
    host: String,
    port: u16,
) -> Result<(), ModelError> {
    let id = TrackId(project.next_strip_id());
    let mut output = Track::new_output(id, name, host, port);
    // Outputs cycle the palette by position; the track/group swatch counter stays put.
    output.colour = LIVE_CLIP_PALETTE[project.outputs.len() % LIVE_CLIP_PALETTE.len()];
    project.outputs.push(output);
    Ok(())
}

/// Removing an output track also takes it out of every route.
pub(super) fn remove_output(project: &mut Project, output: TrackId) -> Result<(), ModelError> {
    if !project.outputs.iter().any(|o| o.id == output) {
        return Err(unknown_output());
    }
    project.outputs.retain(|o| o.id != output);
    for track in &mut project.tracks {
        if let Route::Outputs(ids) = &mut track.route {
            ids.retain(|id| *id != output);
        }
    }
    Ok(())
}

/// Rename any stored track without changing its other settings.
pub(super) fn rename_track(
    project: &mut Project,
    track: TrackId,
    name: String,
) -> Result<(), ModelError> {
    let found = get_track(project, track)?;
    if !found.capabilities().renamable {
        return Err(ModelError("this track cannot be renamed".into()));
    }
    found.name = name;
    Ok(())
}

pub(super) fn set_output_stage(
    project: &mut Project,
    output: TrackId,
    stage: OutputStage,
) -> Result<(), ModelError> {
    stage.validate()?;
    output_mut(project, output)?
        .output
        .as_mut()
        .ok_or_else(|| ModelError("set a host before changing the output stage".into()))?
        .stage = stage;
    Ok(())
}

pub(super) fn set_route(
    project: &mut Project,
    owner: TrackId,
    route: Route,
) -> Result<(), ModelError> {
    let track = get_track(project, owner)?;
    if !track.capabilities().routes {
        return Err(no_route());
    }
    track.route = route;
    Ok(())
}

/// Validate each entry before publishing any changes, including entries overwritten later
/// in the batch. Full model validation also checks route-dependent timing bounds.
pub(super) fn set_routes(
    project: &mut Project,
    routes: Vec<(TrackId, Route)>,
) -> Result<(), ModelError> {
    let outputs = project.destinations().map(|output| output.id).collect();
    for (owner, route) in &routes {
        let track = project
            .track(*owner)
            .ok_or_else(|| ModelError("unknown track".into()))?;
        if !track.capabilities().routes {
            return Err(no_route());
        }
        if track.is_group() && track.group.is_none() && *route == Route::Group {
            return Err(ModelError("group route cannot inherit a group".into()));
        }
        crate::model::validate_route(route, &outputs)?;
    }
    let mut candidate = project.clone();
    for (owner, route) in routes {
        set_route(&mut candidate, owner, route)?;
    }
    candidate.validate_stored()?;
    *project = candidate;
    Ok(())
}

/// Toggle by agreement across all targets, using their routes before any changes. Inherited
/// destinations are not explicit ticks: choosing one replaces Group with just that destination.
/// Routes follow checklist order (Main, then outputs), regardless of click or previous route order.
pub(super) fn toggle_output_in_route(
    project: &mut Project,
    tracks: Vec<TrackId>,
    output: TrackId,
) -> Result<(), ModelError> {
    if project.destination(output).is_none() {
        return Err(unknown_output());
    }
    for id in &tracks {
        let track = project
            .track(*id)
            .ok_or_else(|| ModelError("unknown track".into()))?;
        if !track.capabilities().routes {
            return Err(no_route());
        }
    }
    let untick = tracks
        .iter()
        .all(|id| matches!(&project[*id].route, Route::Outputs(ids) if ids.contains(&output)));
    let routes = tracks
        .iter()
        .map(|id| {
            let route = match &project[*id].route {
                Route::Group => Route::single(output),
                Route::Outputs(ids) => Route::Outputs(
                    project
                        .destinations()
                        .filter(|destination| {
                            if destination.id == output {
                                !untick
                            } else {
                                ids.contains(&destination.id)
                            }
                        })
                        .map(|destination| destination.id)
                        .collect(),
                ),
            };
            (*id, route)
        })
        .collect();
    set_routes(project, routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{ProjectEdit, apply};

    fn project() -> Project {
        let mut project = Project::default();
        for name in ["One", "Two"] {
            apply(&mut project, ProjectEdit::AddTrack { name: name.into() }).unwrap();
        }
        add_output(&mut project, "Stage".into(), "127.0.0.1".into(), 9000).unwrap();
        project.validate().unwrap();
        project
    }

    #[test]
    fn batch_sets_all_routes_including_main_and_group() {
        let mut p = project();
        let (one, two, output, main) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id, p.main.id);
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![one, two],
            },
        )
        .unwrap();
        apply(
            &mut p,
            ProjectEdit::SetTrackRoutes {
                routes: vec![
                    (one, Route::Outputs(vec![main, output])),
                    (two, Route::Group),
                ],
            },
        )
        .unwrap();
        assert_eq!(p[one].route, Route::Outputs(vec![main, output]));
        assert_eq!(p[two].route, Route::Group);
    }

    #[test]
    fn invalid_batch_entries_leave_the_entire_project_unchanged() {
        let p = project();
        let (one, two, output, main) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id, p.main.id);
        for invalid in [
            (TrackId(999), Route::empty()),
            (output, Route::empty()),
            (main, Route::empty()),
            (two, Route::single(TrackId(999))),
            (two, Route::Outputs(vec![output, output])),
        ] {
            let mut candidate = p.clone();
            assert!(
                apply(
                    &mut candidate,
                    ProjectEdit::SetTrackRoutes {
                        routes: vec![(one, Route::single(output)), invalid],
                    }
                )
                .is_err()
            );
            assert_eq!(candidate, p);
        }
    }

    #[test]
    fn invalid_entry_cannot_be_hidden_by_a_later_entry_for_the_same_owner() {
        let mut p = project();
        let before = p.clone();
        let one = p.tracks[0].id;
        assert!(
            set_routes(
                &mut p,
                vec![(one, Route::single(TrackId(999))), (one, Route::empty()),]
            )
            .is_err()
        );
        assert_eq!(p, before);
    }

    #[test]
    fn batch_validates_combined_timing_after_all_routes_are_set() {
        let mut p = project();
        let (one, two, output, main) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id, p.main.id);
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![one, two],
            },
        )
        .unwrap();
        let group = p.group_of(one).unwrap().id;
        p[one].delay_ms = 600.0;
        p[one].route = Route::Group;
        p.outputs[0].delay_ms = 500.0;
        p.validate().unwrap();
        let before = p.clone();
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetTrackRoutes {
                    routes: vec![(two, Route::empty()), (group, Route::single(output))],
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
        // Routing the member to Main removes its inherited latency in the same batch.
        apply(
            &mut p,
            ProjectEdit::SetTrackRoutes {
                routes: vec![(group, Route::single(output)), (one, Route::single(main))],
            },
        )
        .unwrap();
        assert_eq!(p[group].route, Route::single(output));
        assert_eq!(p[one].route, Route::single(main));
    }

    #[test]
    fn group_toggle_replaces_inheritance_even_when_destination_was_inherited() {
        let mut p = project();
        let one = p.tracks[0].id;
        let two = p.tracks[1].id;
        let output = p.outputs[0].id;
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![one, two],
            },
        )
        .unwrap();
        let group = p.group_of(one).unwrap().id;
        set_route(&mut p, group, Route::single(output)).unwrap();
        for destination in [output, p.main.id] {
            set_route(&mut p, one, Route::Group).unwrap();
            apply(
                &mut p,
                ProjectEdit::ToggleOutputInRoute {
                    tracks: vec![one],
                    output: destination,
                },
            )
            .unwrap();
            assert_eq!(p[one].route, Route::single(destination));
        }
    }

    #[test]
    fn mixed_toggle_ticks_every_target_then_agreement_unticks_every_target() {
        let mut p = project();
        let (one, two, output, main) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id, p.main.id);
        p[two].route = Route::single(output);
        for expected in [
            [Route::Outputs(vec![main, output]), Route::single(output)],
            [Route::single(main), Route::empty()],
        ] {
            apply(
                &mut p,
                ProjectEdit::ToggleOutputInRoute {
                    tracks: vec![one, two],
                    output,
                },
            )
            .unwrap();
            assert_eq!([p[one].route.clone(), p[two].route.clone()], expected);
        }
    }

    #[test]
    fn inherited_ticks_do_not_agree_with_explicit_ticks() {
        let mut p = project();
        let (one, two, output, main) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id, p.main.id);
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![one, two],
            },
        )
        .unwrap();
        let group = p.group_of(one).unwrap().id;
        p[group].route = Route::Outputs(vec![main, output]);
        // Both targets send to output, but an inherited destination is not a tick.
        for second in [Route::Group, Route::Outputs(vec![main, output])] {
            p[one].route = Route::Group;
            p[two].route = second.clone();
            apply(
                &mut p,
                ProjectEdit::ToggleOutputInRoute {
                    tracks: vec![one, two],
                    output,
                },
            )
            .unwrap();
            assert_eq!(p[one].route, Route::single(output));
            assert_eq!(
                p[two].route,
                if second == Route::Group {
                    Route::single(output)
                } else {
                    second
                }
            );
            assert_eq!(p[group].route, Route::Outputs(vec![main, output]));
        }
    }

    #[test]
    fn group_and_member_toggle_use_routes_before_the_edit() {
        let mut p = project();
        let (one, two, output) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id);
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![one, two],
            },
        )
        .unwrap();
        let group = p.group_of(one).unwrap().id;
        p[group].route = Route::single(output);
        p[one].route = Route::Group;
        apply(
            &mut p,
            ProjectEdit::ToggleOutputInRoute {
                tracks: vec![group, one],
                output,
            },
        )
        .unwrap();
        assert_eq!(p[group].route, Route::single(output));
        assert_eq!(p[one].route, Route::single(output));
        assert_eq!(p[two].route, Route::Group);
    }

    #[test]
    fn toggles_store_main_then_current_project_output_order() {
        let mut p = project();
        add_output(&mut p, "Video".into(), "127.0.0.1".into(), 9001).unwrap();
        let (one, output, video, main) =
            (p.tracks[0].id, p.outputs[0].id, p.outputs[1].id, p.main.id);
        p[one].route = Route::empty();
        // Reverse clicks, including removing and reticking Main, keep checklist order.
        for (destination, expected) in [
            (video, vec![video]),
            (output, vec![output, video]),
            (main, vec![main, output, video]),
            (main, vec![output, video]),
            (main, vec![main, output, video]),
        ] {
            apply(
                &mut p,
                ProjectEdit::ToggleOutputInRoute {
                    tracks: vec![one],
                    output: destination,
                },
            )
            .unwrap();
            assert_eq!(p[one].route, Route::Outputs(expected));
        }
        p.outputs.reverse();
        apply(
            &mut p,
            ProjectEdit::ToggleOutputInRoute {
                tracks: vec![one],
                output: main,
            },
        )
        .unwrap();
        assert_eq!(p[one].route, Route::Outputs(vec![video, output]));
    }

    #[test]
    fn repeated_targets_toggle_once_and_leave_other_tracks_alone() {
        let mut p = project();
        let (one, two, output, main) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id, p.main.id);
        apply(
            &mut p,
            ProjectEdit::ToggleOutputInRoute {
                tracks: vec![one, one],
                output,
            },
        )
        .unwrap();
        assert_eq!(p[one].route, Route::Outputs(vec![main, output]));
        assert_eq!(p[two].route, Route::single(main));
    }

    #[test]
    fn invalid_toggle_target_or_destination_rejects_the_entire_edit() {
        let p = project();
        let (one, output, main) = (p.tracks[0].id, p.outputs[0].id, p.main.id);
        for (tracks, destination) in [
            (vec![one, TrackId(999)], output),
            (vec![one, output], output),
            (vec![one, main], output),
            (vec![one], TrackId(999)),
            (vec![one], one),
        ] {
            let mut candidate = p.clone();
            assert!(
                apply(
                    &mut candidate,
                    ProjectEdit::ToggleOutputInRoute {
                        tracks,
                        output: destination,
                    },
                )
                .is_err()
            );
            assert_eq!(candidate, p);
        }
    }

    #[test]
    fn toggle_validates_combined_timing_before_publishing_any_route() {
        let mut p = project();
        let (one, two, output) = (p.tracks[0].id, p.tracks[1].id, p.outputs[0].id);
        p[two].delay_ms = 600.0;
        p.outputs[0].delay_ms = 500.0;
        p.validate().unwrap();
        let before = p.clone();
        assert!(
            apply(
                &mut p,
                ProjectEdit::ToggleOutputInRoute {
                    tracks: vec![one, two],
                    output,
                },
            )
            .is_err()
        );
        assert_eq!(p, before);
    }

    #[test]
    fn main_toggles_freely_with_outputs_even_when_unset() {
        let mut p = project();
        p.main.output = None;
        let one = p.tracks[0].id;
        let main = p.main.id;
        let output = p.outputs[0].id;
        set_route(&mut p, one, Route::single(output)).unwrap();
        for expected in [vec![main, output], vec![output]] {
            apply(
                &mut p,
                ProjectEdit::ToggleOutputInRoute {
                    tracks: vec![one],
                    output: main,
                },
            )
            .unwrap();
            assert_eq!(p[one].route, Route::Outputs(expected));
        }
    }
}
