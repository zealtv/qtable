//! Model-to-presentation adapter for path fields and their Info View breakdowns.
use crate::control::path_field::{Ownership, Prefix, Segment};
use qtable_engine::model::{Ancestor, DeviceKind, PathLevel, Project, Track, TrackId};

pub(crate) fn inherited(
    project: &Project,
    source: TrackId,
    destination: TrackId,
    level: PathLevel,
) -> Option<Vec<Segment>> {
    let own = project.track(source)?;
    let output = project.destination(destination)?;
    output.destination()?;
    if level == PathLevel::Track && source == destination {
        return Some(Vec::new());
    }
    let mut segments = vec![Segment::new(
        "Output",
        output.path.as_str(),
        Ownership::Inherited,
    )];
    let mut groups: Vec<_> = project
        .ancestors(source)
        .filter_map(Ancestor::group)
        .collect();
    groups.reverse();
    segments.extend(
        groups
            .into_iter()
            .map(|group| Segment::new("Group", group.path.as_str(), Ownership::Inherited)),
    );
    if level != PathLevel::Track && source != destination {
        segments.push(Segment::new(
            "Track",
            own.path.as_str(),
            Ownership::Inherited,
        ));
    }
    if let PathLevel::Device(id) = level {
        let device = own.device(id)?;
        match &device.kind {
            DeviceKind::Target(target) => segments.push(Segment::new(
                "Target",
                target.path.as_str(),
                Ownership::Inherited,
            )),
        }
    }
    // The closest owner is the parent even when its piece is empty.
    if let Some(parent) = segments.last_mut() {
        parent.ownership = Ownership::Parent;
    }
    Some(segments)
}

pub(crate) fn paths(
    project: &Project,
    source: TrackId,
    level: PathLevel,
) -> Vec<(&Track, Vec<Segment>)> {
    project
        .path_destinations(source)
        .into_iter()
        .filter_map(|destination| {
            inherited(project, source, destination.id, level)
                .map(|segments| (destination, segments))
        })
        .collect()
}

impl Prefix {
    pub(crate) fn of(project: &Project, source: TrackId, level: PathLevel, absolute: bool) -> Self {
        if absolute {
            return Self::default();
        }
        let paths = paths(project, source, level);
        let mut distinct = Vec::new();
        for (_, segments) in &paths {
            let text: String = segments.iter().map(|s| s.path.as_str()).collect();
            if !distinct.contains(&text) {
                distinct.push(text);
            }
        }
        Self {
            segments: paths.first().map(|(_, s)| s.clone()).unwrap_or_default(),
            more: distinct.len().saturating_sub(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::model::{DeviceId, Route};

    #[test]
    fn levels_match_engine_composition_for_tracks_devices_and_destinations() {
        let mut project = Project::default();
        let output = Track::new_output(TrackId(20), "Out", "127.0.0.1", 9000).with_path("/synth");
        let mut group = Track::new_group(TrackId(10), "Drums", 0).with_path("/drums");
        group.route = Route::single(output.id);
        let mut track = Track::new(TrackId(1), "Kick", 0).with_path("/kick");
        track.group = Some(group.id);
        track.route = Route::Group;
        let mut device = qtable_engine::model::NewDevice::Target.build(DeviceId(1));
        let target = device.target_mut().unwrap();
        target.path = qtable_engine::model::Path::new("/mypath").unwrap();
        track.devices.push(device);
        project.outputs.push(output);
        project.tracks = vec![group, track];
        for (source, level) in [
            (TrackId(1), PathLevel::Track),
            (TrackId(1), PathLevel::Under),
            (TrackId(1), PathLevel::Device(DeviceId(1))),
            (TrackId(10), PathLevel::Track),
            (TrackId(20), PathLevel::Track),
            (TrackId(20), PathLevel::Under),
        ] {
            let prefix = Prefix::of(&project, source, level, false);
            assert_eq!(
                prefix.text(),
                project.inherited_path(source, TrackId(20), level).unwrap()
            );
        }
        let prefix = Prefix::of(&project, TrackId(1), PathLevel::Device(DeviceId(1)), false);
        assert_eq!(
            prefix.segments,
            vec![
                Segment::new("Output", "/synth", Ownership::Inherited),
                Segment::new("Group", "/drums", Ownership::Inherited),
                Segment::new("Track", "/kick", Ownership::Inherited),
                Segment::new("Target", "/mypath", Ownership::Parent),
            ]
        );
        let strip = Prefix::of(&project, TrackId(1), PathLevel::Track, false);
        assert_eq!(strip.segments[1].ownership, Ownership::Parent);
        assert_eq!(strip.segments[1].label, "Group");
        assert_eq!(
            Prefix::of(&project, TrackId(1), PathLevel::Device(DeviceId(1)), true),
            Prefix::default()
        );
    }
}
