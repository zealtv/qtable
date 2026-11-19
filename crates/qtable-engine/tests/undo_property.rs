//! Property test for the undo history: from a seeded set,
//! apply random valid edits across every `ProjectEdit` variant; undo all and compare with the
//! start, redo all and compare with the end.
//!
//! The generator matches exhaustively on `Kind`, and `kind_of` matches exhaustively on
//! `ProjectEdit`, so a new edit fails to compile here until it has a case.
use qtable_engine::{
    edit::{Carried, EnvelopeEdit, LaneEdit, MappingEdit, ProjectEdit, TargetEdit},
    engine::{Command, Engine, EngineError},
    model::*,
};
use std::collections::HashSet;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn flip(&mut self) -> bool {
        self.next() & 1 == 1
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len())])
    }
    fn word(&mut self) -> String {
        format!("w{}", self.below(10_000))
    }
    fn unit(&mut self) -> f64 {
        self.below(1001) as f64 / 1000.0
    }
}

macro_rules! kinds {
    ($($kind:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        enum Kind { $($kind),* }
        const ALL: &[Kind] = &[$(Kind::$kind),*];
    };
}
kinds!(
    EditMapping,
    EditEnvelope,
    AddDevice,
    PasteDevices,
    DuplicateDevices,
    MoveDevice,
    RemoveDevices,
    RenameDevice,
    SetDeviceExpanded,
    SetDevicesActive,
    EditTarget,
    SetParameter,
    SetParameters,
    SetParameterOutput,
    SetTrackPath,
    AddOutput,
    SetDestination,
    SetTrackDelay,
    SetOutputStage,
    SetTrackRoutes,
    ToggleOutputInRoute,
    AddTrack,
    RemoveTracks,
    RenameTrack,
    RecolourTrack,
    MoveTrack,
    PlaceTrack,
    PlaceTracks,
    PasteTracks,
    CreateGroup,
    Ungroup,
    UngroupTracks,
    SetGroupFolded,
    AddScene,
    RemoveScene,
    RemoveScenes,
    RenameScene,
    SetSceneFollowActions,
    MoveScenes,
    MoveScene,
    CreateClip,
    DeleteClip,
    DuplicateClip,
    PasteClip,
    MoveClip,
    RecolourClip,
    RenameClip,
    SetClipMarkers,
    SetClipLength,
    SetClipLooping,
    SetClipLegato,
    SetClipQuantisation,
    SetClipFollowActions,
    AddEvent,
    RemoveEvent,
    UpdateEvent,
    EditLane,
    SetSlotStopButton,
    SetActive,
    SetOutputSolo,
    SetSolo,
);

fn kind_of(edit: &ProjectEdit) -> Kind {
    match edit {
        ProjectEdit::EditMapping(_) => Kind::EditMapping,
        ProjectEdit::EditEnvelope { .. } => Kind::EditEnvelope,
        ProjectEdit::AddDevice { .. } => Kind::AddDevice,
        ProjectEdit::PasteDevices { .. } => Kind::PasteDevices,
        ProjectEdit::DuplicateDevices { .. } => Kind::DuplicateDevices,
        ProjectEdit::MoveDevice { .. } => Kind::MoveDevice,
        ProjectEdit::RemoveDevices { .. } => Kind::RemoveDevices,
        ProjectEdit::RenameDevice { .. } => Kind::RenameDevice,
        ProjectEdit::SetDeviceExpanded { .. } => Kind::SetDeviceExpanded,
        ProjectEdit::SetDevicesActive { .. } => Kind::SetDevicesActive,
        ProjectEdit::EditTarget { .. } => Kind::EditTarget,
        ProjectEdit::SetParameter { .. } => Kind::SetParameter,
        ProjectEdit::SetParameters(_) => Kind::SetParameters,
        ProjectEdit::SetParameterOutput { .. } => Kind::SetParameterOutput,
        ProjectEdit::SetTrackPath { .. } => Kind::SetTrackPath,
        ProjectEdit::AddOutput { .. } => Kind::AddOutput,
        ProjectEdit::SetDestination { .. } => Kind::SetDestination,
        ProjectEdit::SetTrackDelay { .. } => Kind::SetTrackDelay,
        ProjectEdit::SetOutputStage { .. } => Kind::SetOutputStage,
        ProjectEdit::SetTrackRoutes { .. } => Kind::SetTrackRoutes,
        ProjectEdit::ToggleOutputInRoute { .. } => Kind::ToggleOutputInRoute,
        ProjectEdit::AddTrack { .. } => Kind::AddTrack,
        ProjectEdit::RemoveTracks { .. } => Kind::RemoveTracks,
        ProjectEdit::RenameTrack { .. } => Kind::RenameTrack,
        ProjectEdit::RecolourTrack { .. } => Kind::RecolourTrack,
        ProjectEdit::MoveTrack { .. } => Kind::MoveTrack,
        ProjectEdit::PlaceTrack { .. } => Kind::PlaceTrack,
        ProjectEdit::PlaceTracks { .. } => Kind::PlaceTracks,
        ProjectEdit::PasteTracks { .. } => Kind::PasteTracks,
        ProjectEdit::CreateGroup { .. } => Kind::CreateGroup,
        ProjectEdit::Ungroup { .. } => Kind::Ungroup,
        ProjectEdit::UngroupTracks { .. } => Kind::UngroupTracks,
        ProjectEdit::SetGroupFolded { .. } => Kind::SetGroupFolded,
        ProjectEdit::AddScene { .. } => Kind::AddScene,
        ProjectEdit::RemoveScene { .. } => Kind::RemoveScene,
        ProjectEdit::RemoveScenes { .. } => Kind::RemoveScenes,
        ProjectEdit::RenameScene { .. } => Kind::RenameScene,
        ProjectEdit::SetSceneFollowActions { .. } => Kind::SetSceneFollowActions,
        ProjectEdit::MoveScenes { .. } => Kind::MoveScenes,
        ProjectEdit::MoveScene { .. } => Kind::MoveScene,
        ProjectEdit::CreateClip { .. } => Kind::CreateClip,
        ProjectEdit::DeleteClip { .. } => Kind::DeleteClip,
        ProjectEdit::DuplicateClip { .. } => Kind::DuplicateClip,
        ProjectEdit::PasteClip { .. } => Kind::PasteClip,
        ProjectEdit::MoveClip { .. } => Kind::MoveClip,
        ProjectEdit::RecolourClip { .. } => Kind::RecolourClip,
        ProjectEdit::RenameClip { .. } => Kind::RenameClip,
        ProjectEdit::SetClipMarkers { .. } => Kind::SetClipMarkers,
        ProjectEdit::SetClipLength { .. } => Kind::SetClipLength,
        ProjectEdit::SetClipLooping { .. } => Kind::SetClipLooping,
        ProjectEdit::SetClipLegato { .. } => Kind::SetClipLegato,
        ProjectEdit::SetClipQuantisation { .. } => Kind::SetClipQuantisation,
        ProjectEdit::SetClipFollowActions { .. } => Kind::SetClipFollowActions,
        ProjectEdit::AddEvent { .. } => Kind::AddEvent,
        ProjectEdit::RemoveEvent { .. } => Kind::RemoveEvent,
        ProjectEdit::UpdateEvent { .. } => Kind::UpdateEvent,
        ProjectEdit::EditLane { .. } => Kind::EditLane,
        ProjectEdit::SetSlotStopButton { .. } => Kind::SetSlotStopButton,
        ProjectEdit::SetActive { .. } => Kind::SetActive,
        ProjectEdit::SetOutputSolo { .. } => Kind::SetOutputSolo,
        ProjectEdit::SetSolo { .. } => Kind::SetSolo,
    }
}

fn is_view_state(kind: Kind) -> bool {
    matches!(kind, Kind::SetGroupFolded | Kind::SetDeviceExpanded)
}

/// Mapping edits make no step and no undo or redo restores mappings (2026-11-12).
fn is_mapping(kind: Kind) -> bool {
    kind == Kind::EditMapping
}

/// What the generator reads off the current set.
struct View<'a>(&'a Project);
impl View<'_> {
    fn clip_tracks(&self) -> Vec<TrackId> {
        self.0.clip_tracks().map(|t| t.id).collect()
    }
    fn groups(&self) -> Vec<TrackId> {
        self.0
            .tracks
            .iter()
            .filter(|t| t.is_group())
            .map(|t| t.id)
            .collect()
    }
    fn strips(&self) -> Vec<TrackId> {
        self.0.tracks.iter().map(|t| t.id).collect()
    }
    fn outputs(&self) -> Vec<TrackId> {
        self.0.outputs.iter().map(|t| t.id).collect()
    }
    fn scenes(&self) -> Vec<SceneId> {
        self.0.scenes.iter().map(|s| s.id).collect()
    }
    fn devices(&self) -> Vec<(TrackId, DeviceId)> {
        self.0
            .tracks
            .iter()
            .flat_map(|t| t.devices.iter().map(|d| (t.id, d.id)))
            .collect()
    }
    fn clips(&self) -> Vec<(TrackId, usize)> {
        self.0
            .clip_tracks()
            .flat_map(|t| {
                t.slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.clip.is_some())
                    .map(|(i, _)| (t.id, i))
            })
            .collect()
    }
    fn empty_slots(&self) -> Vec<(TrackId, usize)> {
        self.0
            .clip_tracks()
            .flat_map(|t| {
                t.slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.clip.is_none())
                    .map(|(i, _)| (t.id, i))
            })
            .collect()
    }
    fn slots(&self) -> Vec<(TrackId, usize)> {
        self.0
            .clip_tracks()
            .flat_map(|t| (0..t.slots.len()).map(|i| (t.id, i)))
            .collect()
    }
}

fn volume(track: TrackId) -> ArgumentRef {
    let _ = track;
    ArgumentRef::volume()
}

fn generate(kind: Kind, p: &Project, rng: &mut Rng) -> Option<ProjectEdit> {
    let v = View(p);
    let tracks = v.clip_tracks();
    let strips = v.strips();
    let outputs = v.outputs();
    let scenes = v.scenes();
    let clips = v.clips();
    let colour = |rng: &mut Rng| LIVE_CLIP_PALETTE[rng.below(LIVE_CLIP_PALETTE.len())];
    Some(match kind {
        Kind::EditMapping => {
            let existing: Vec<_> = p.mappings.iter().map(|m| m.id).collect();
            let id = rng.pick(&existing).copied();
            ProjectEdit::EditMapping(match (rng.below(5), id) {
                (1, Some(mapping)) => MappingEdit::Remove { mapping },
                (2, Some(mapping)) => MappingEdit::SetRange {
                    mapping,
                    min: 0.1,
                    max: 0.1 + rng.unit() * 0.8,
                },
                (3, Some(mapping)) => MappingEdit::SetMode {
                    mapping,
                    mode: if rng.flip() {
                        SourceMode::Absolute
                    } else {
                        SourceMode::Relative(RelativeEncoding::SignedBit)
                    },
                },
                (4, Some(mapping)) => MappingEdit::SetTakeover {
                    mapping,
                    takeover: if rng.flip() {
                        Takeover::None
                    } else {
                        Takeover::PickUp
                    },
                },
                _ => {
                    let track = *rng.pick(&tracks)?;
                    MappingEdit::Add {
                        source: Source::Key(KeySource {
                            key: KeyName(((b'a' + rng.below(26) as u8) as char).to_string()),
                        }),
                        control: ControlRef::Argument(volume(track).on(track)),
                        min: 0.0,
                        max: 1.0,
                    }
                }
            })
        }
        Kind::EditEnvelope => {
            let (track, slot) = *rng.pick(&clips)?;
            let target = volume(track);
            let clip = p.clip(track, slot)?;
            let has = clip
                .lanes
                .iter()
                .any(|l| l.envelope().is_some_and(|e| e.target == target));
            let edit = match (has, rng.below(3)) {
                (false, _) => EnvelopeEdit::Add { target },
                (true, 0) => EnvelopeEdit::Remove { target },
                (true, 1) => EnvelopeEdit::Move { target, to: 0 },
                (true, _) => EnvelopeEdit::SetPoints {
                    target,
                    points: vec![
                        Breakpoint {
                            time: BeatTime(0),
                            value: rng.unit(),
                            curve: 0.0,
                        },
                        Breakpoint {
                            time: BeatTime(480),
                            value: rng.unit(),
                            curve: 0.0,
                        },
                    ],
                },
            };
            ProjectEdit::EditEnvelope { track, slot, edit }
        }
        Kind::AddDevice => ProjectEdit::AddDevice {
            track: *rng.pick(&tracks)?,
            kind: NewDevice::Target,
            before: None,
        },
        Kind::PasteDevices => {
            let (_, device) = *rng.pick(&v.devices())?;
            ProjectEdit::PasteDevices {
                track: *rng.pick(&tracks)?,
                devices: vec![p.device(device)?.1.clone()],
                before: None,
            }
        }
        Kind::DuplicateDevices => ProjectEdit::DuplicateDevices {
            devices: vec![rng.pick(&v.devices())?.1],
        },
        Kind::MoveDevice => ProjectEdit::MoveDevice {
            device: rng.pick(&v.devices())?.1,
            before: rng.pick(&v.devices()).map(|d| d.1),
        },
        Kind::RemoveDevices => ProjectEdit::RemoveDevices {
            devices: vec![rng.pick(&v.devices())?.1],
        },
        Kind::RenameDevice => ProjectEdit::RenameDevice {
            device: rng.pick(&v.devices())?.1,
            name: rng.word(),
        },
        Kind::SetDeviceExpanded => ProjectEdit::SetDeviceExpanded {
            device: rng.pick(&v.devices())?.1,
            expanded: rng.flip(),
        },
        Kind::SetDevicesActive => ProjectEdit::SetDevicesActive {
            devices: vec![rng.pick(&v.devices())?.1],
            active: rng.flip(),
        },
        Kind::EditTarget => {
            let (track, device) = *rng.pick(&v.devices())?;
            let target = p.device(device)?.1.target().unwrap();
            let entries: Vec<EntryKey> = target
                .events
                .iter()
                .map(|e| e.key.clone())
                .chain(target.parameters.iter().map(|e| e.key.clone()))
                .collect();
            let _ = track;
            let path = OscAddress::Relative(format!("/{}", rng.word()));
            let edit = match (rng.below(5), rng.pick(&entries)) {
                (1, _) => TargetEdit::AddParameter {
                    path,
                    name: rng.word(),
                },
                (2, _) => TargetEdit::SetPath(rng.word()),
                (3, Some(key)) => TargetEdit::SetName {
                    key: key.clone(),
                    name: rng.word(),
                },
                (4, Some(key)) => TargetEdit::RemoveEntries {
                    keys: vec![key.clone()],
                },
                _ => TargetEdit::AddEvent {
                    path,
                    name: rng.word(),
                },
            };
            ProjectEdit::EditTarget { device, edit }
        }
        Kind::SetParameter => {
            let track = *rng.pick(&strips)?;
            ProjectEdit::SetParameter {
                target: ParameterRef::volume(track),
                values: vec![(FieldKey("value".into()), rng.unit())],
            }
        }
        Kind::SetParameters => {
            let first = *rng.pick(&strips)?;
            let second = *rng.pick(&strips)?;
            if first == second {
                return None;
            }
            ProjectEdit::SetParameters(
                [first, second]
                    .into_iter()
                    .map(|t| {
                        (
                            ParameterRef::volume(t),
                            vec![(FieldKey("value".into()), rng.unit())],
                        )
                    })
                    .collect(),
            )
        }
        Kind::SetParameterOutput => {
            let track = *rng.pick(&strips)?;
            let target = ParameterRef::volume(track);
            let mut output = p.parameter(&target)?.output.into_owned();
            output.enabled = rng.flip();
            output.address = format!("/{}", rng.word());
            ProjectEdit::SetParameterOutput { target, output }
        }
        Kind::SetTrackPath => ProjectEdit::SetTrackPath {
            track: *rng.pick(&strips)?,
            path: format!("/{}", rng.word()),
        },
        Kind::AddOutput => ProjectEdit::AddOutput {
            name: rng.word(),
            host: "127.0.0.1".into(),
            port: 9100 + rng.below(100) as u16,
        },
        Kind::SetDestination => {
            let output = *rng.pick(&outputs)?;
            let mut destination = p.track(output)?.output.clone()?;
            destination.destination.port = 9200 + rng.below(100) as u16;
            ProjectEdit::SetDestination {
                output,
                destination: Some(destination),
            }
        }
        Kind::SetTrackDelay => ProjectEdit::SetTrackDelay {
            track: *rng.pick(&strips)?,
            delay_ms: rng.below(50) as f64,
        },
        Kind::SetOutputStage => {
            let output = *rng.pick(&outputs)?;
            let mut stage = p.track(output)?.output.clone()?.stage;
            stage.bundle = !stage.bundle;
            ProjectEdit::SetOutputStage { output, stage }
        }
        Kind::SetTrackRoutes => ProjectEdit::SetTrackRoutes {
            routes: vec![(*rng.pick(&strips)?, Route::single(*rng.pick(&outputs)?))],
        },
        Kind::ToggleOutputInRoute => ProjectEdit::ToggleOutputInRoute {
            tracks: vec![*rng.pick(&strips)?],
            output: *rng.pick(&outputs)?,
        },
        Kind::AddTrack => ProjectEdit::AddTrack { name: rng.word() },
        Kind::RemoveTracks => ProjectEdit::RemoveTracks {
            tracks: vec![*rng.pick(&strips)?],
        },
        Kind::RenameTrack => ProjectEdit::RenameTrack {
            track: *rng.pick(&strips)?,
            name: rng.word(),
        },
        Kind::RecolourTrack => ProjectEdit::RecolourTrack {
            track: *rng.pick(&strips)?,
            colour: colour(rng),
        },
        Kind::MoveTrack => ProjectEdit::MoveTrack {
            from: rng.below(p.tracks.len()),
            to: rng.below(p.tracks.len()),
        },
        Kind::PlaceTrack => ProjectEdit::PlaceTrack {
            track: *rng.pick(&strips)?,
            before: rng.pick(&strips).copied(),
            group: rng.pick(&v.groups()).copied().filter(|_| rng.flip()),
        },
        Kind::PlaceTracks => ProjectEdit::PlaceTracks {
            tracks: vec![*rng.pick(&strips)?],
            before: rng.pick(&strips).copied().filter(|_| rng.flip()),
            group: rng.pick(&v.groups()).copied().filter(|_| rng.flip()),
        },
        Kind::PasteTracks => {
            let track = p.track(*rng.pick(&tracks)?)?.clone();
            ProjectEdit::PasteTracks {
                tracks: vec![track],
                before: None,
                group: None,
            }
        }
        Kind::CreateGroup => ProjectEdit::CreateGroup {
            tracks: tracks
                .iter()
                .copied()
                .filter(|_| rng.flip())
                .take(2)
                .collect(),
        },
        Kind::Ungroup => ProjectEdit::Ungroup {
            group: *rng.pick(&v.groups())?,
        },
        Kind::UngroupTracks => ProjectEdit::UngroupTracks {
            groups: vec![*rng.pick(&v.groups())?],
        },
        Kind::SetGroupFolded => ProjectEdit::SetGroupFolded {
            group: *rng.pick(&v.groups())?,
            folded: rng.flip(),
        },
        Kind::AddScene => ProjectEdit::AddScene {
            at: rng.below(scenes.len() + 1),
            name: rng.word(),
        },
        Kind::RemoveScene => ProjectEdit::RemoveScene {
            scene: *rng.pick(&scenes)?,
        },
        Kind::RemoveScenes => ProjectEdit::RemoveScenes {
            scenes: vec![*rng.pick(&scenes)?],
        },
        Kind::RenameScene => ProjectEdit::RenameScene {
            scene: *rng.pick(&scenes)?,
            name: rng.word(),
        },
        Kind::SetSceneFollowActions => ProjectEdit::SetSceneFollowActions {
            scene: *rng.pick(&scenes)?,
            follow_actions: SceneFollowActions {
                time: BeatTime(960 * (1 + rng.below(8) as u64)),
                ..Default::default()
            },
        },
        Kind::MoveScenes => ProjectEdit::MoveScenes {
            scenes: vec![*rng.pick(&scenes)?],
            boundary: rng.below(scenes.len() + 1),
        },
        Kind::MoveScene => ProjectEdit::MoveScene {
            from: rng.below(scenes.len()),
            to: rng.below(scenes.len()),
        },
        Kind::CreateClip => {
            let (track, slot) = *rng.pick(&v.empty_slots())?;
            ProjectEdit::CreateClip {
                track,
                slot,
                length: BeatTime(960 * (1 + rng.below(8) as u64)),
            }
        }
        Kind::DeleteClip => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::DeleteClip { track, slot }
        }
        Kind::DuplicateClip => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::DuplicateClip { track, slot }
        }
        Kind::PasteClip => {
            let (from, at) = (*rng.pick(&clips)?, *rng.pick(&v.slots())?);
            let clip = p.clip(from.0, from.1)?.clone();
            ProjectEdit::PasteClip {
                track: at.0,
                slot: at.1,
                carried: Carried::from_clip(p.track(from.0)?, &clip),
                clip: Box::new(clip),
            }
        }
        Kind::MoveClip => ProjectEdit::MoveClip {
            from: *rng.pick(&clips)?,
            to: *rng.pick(&v.slots())?,
        },
        Kind::RecolourClip => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::RecolourClip {
                track,
                slot,
                colour: colour(rng),
            }
        }
        Kind::RenameClip => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::RenameClip {
                track,
                slot,
                name: rng.word(),
            }
        }
        Kind::SetClipMarkers => {
            let (track, slot) = *rng.pick(&clips)?;
            let mut markers = p.clip(track, slot)?.region();
            markers.looping = !markers.looping;
            ProjectEdit::SetClipMarkers {
                track,
                slot,
                markers,
            }
        }
        Kind::SetClipLength => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::SetClipLength {
                track,
                slot,
                length: BeatTime(960 * (1 + rng.below(8) as u64)),
            }
        }
        Kind::SetClipLooping => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::SetClipLooping {
                track,
                slot,
                looping: rng.flip(),
            }
        }
        Kind::SetClipLegato => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::SetClipLegato {
                track,
                slot,
                legato: rng.flip(),
            }
        }
        Kind::SetClipQuantisation => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::SetClipQuantisation {
                track,
                slot,
                quantisation: if rng.flip() {
                    LaunchQuantisation::None
                } else {
                    LaunchQuantisation::Global
                },
            }
        }
        Kind::SetClipFollowActions => {
            let (track, slot) = *rng.pick(&clips)?;
            ProjectEdit::SetClipFollowActions {
                track,
                slot,
                follow_actions: ClipFollowActions {
                    time: BeatTime(960 * (1 + rng.below(8) as u64)),
                    ..Default::default()
                },
            }
        }
        Kind::AddEvent | Kind::UpdateEvent | Kind::RemoveEvent => {
            let (track, slot) = *rng.pick(&clips)?;
            let clip = p.clip(track, slot)?;
            // Hits of any clip on this track play definitions it owns.
            let known: Vec<Event> = p
                .track(track)?
                .slots
                .iter()
                .filter_map(|s| s.clip.as_ref())
                .flat_map(|c| c.events.iter().cloned())
                .collect();
            let mut event = rng.pick(&known)?.clone();
            event.position = BeatTime(rng.below(clip.end.0.max(1) as usize) as u64);
            match kind {
                Kind::AddEvent => ProjectEdit::AddEvent {
                    track,
                    slot,
                    event,
                    carried: None,
                },
                Kind::UpdateEvent => ProjectEdit::UpdateEvent {
                    track,
                    slot,
                    event: rng.below(clip.events.len()),
                    value: event,
                },
                _ => ProjectEdit::RemoveEvent {
                    track,
                    slot,
                    event: rng.below(clip.events.len()),
                },
            }
        }
        Kind::EditLane => {
            let (track, slot) = *rng.pick(&clips)?;
            let lanes = p.clip(track, slot)?.lanes.len();
            let edit = match (rng.below(3), lanes) {
                (1, 1..) => LaneEdit::RemoveMany {
                    lanes: vec![rng.below(lanes)],
                },
                (2, 2..) => LaneEdit::Move {
                    from: 0..1,
                    to: lanes - 1,
                },
                _ => LaneEdit::AddPath(format!("/{}", rng.word())),
            };
            ProjectEdit::EditLane { track, slot, edit }
        }
        Kind::SetSlotStopButton => {
            let (track, slot) = *rng.pick(&v.slots())?;
            ProjectEdit::SetSlotStopButton {
                track,
                slot,
                enabled: rng.flip(),
            }
        }
        Kind::SetActive => ProjectEdit::SetActive {
            strips: vec![*rng.pick(&strips)?],
            active: rng.flip(),
        },
        Kind::SetOutputSolo => ProjectEdit::SetOutputSolo {
            output: *rng.pick(&outputs)?,
            solo: rng.flip(),
            exclusive: rng.flip(),
        },
        Kind::SetSolo => ProjectEdit::SetSolo {
            strips: vec![*rng.pick(&strips)?],
            solo: rng.flip(),
            exclusive: rng.flip(),
        },
    })
}

/// What an undo or redo should leave: `snapshot` with the current fold of every group and the
/// current expanded flag of every device that exists on both sides (the design's carry-over).
fn carried(current: &Project, snapshot: &Project) -> Project {
    let mut result = snapshot.clone();
    // Mappings stay outside the history: the current ones stay, less those whose control the
    // snapshot lacks (as when an edit deletes the control). A row of the snapshot whose control
    // the current set lacks went with that control when the step's change was made, so it
    // comes back with the control, unless the control already has a binding of its source kind
    // (the current mapping wins) or another current mapping holds its id.
    result.mappings = current
        .mappings
        .iter()
        .filter(|row| {
            describe_control(snapshot, &row.control).is_some()
                || describe_control(current, &row.control).is_none()
        })
        .cloned()
        .collect();
    let mut returned = false;
    for row in &snapshot.mappings {
        let went_with_its_control = describe_control(snapshot, &row.control).is_some()
            && describe_control(current, &row.control).is_none();
        let held = result.mappings.iter().any(|m| {
            m.control == row.control
                && std::mem::discriminant(&m.source) == std::mem::discriminant(&row.source)
        });
        if !went_with_its_control || held {
            continue;
        }
        if result.mappings.iter().any(|m| m.id == row.id) {
            continue;
        }
        result.mappings.push(row.clone());
        returned = true;
    }
    if returned {
        result.mappings.sort_by_key(|m| m.id.0);
    }
    let strips = result
        .tracks
        .iter_mut()
        .chain(result.outputs.iter_mut())
        .chain(std::iter::once(&mut result.main));
    for strip in strips {
        if strip.is_group()
            && let Some(now) = current.track(strip.id).filter(|t| t.is_group())
        {
            strip.fold_state = now.fold_state;
        }
        for device in &mut strip.devices {
            if let Some((_, now)) = current.device(device.id) {
                device.expanded = now.expanded;
            }
        }
    }
    result
}

fn seed_set() -> Project {
    qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v10-groups-mappings.ron"
    ))
    .unwrap()
}

/// The transport-bar commands: the other changes that make steps, and the ones the engine and
/// the scheduler each keep a copy of.
fn transport_command(rng: &mut Rng) -> (&'static str, Command) {
    match rng.below(3) {
        0 => ("tempo", Command::SetTempo(60.0 + rng.below(120) as f64)),
        1 => (
            "quantisation",
            Command::SetGlobalQuantisation(if rng.flip() {
                GlobalQuantisation::None
            } else {
                GlobalQuantisation::Value(QuantisationValue::Bars([1, 2, 4, 8][rng.below(4)]))
            }),
        ),
        _ => ("follow", Command::SetFollowActionsEnabled(rng.flip())),
    }
}

/// One random walk of valid edits; returns which edit kinds and transport-bar commands applied.
fn walk(seed: u64, steps: usize, with_view_state: bool) -> (HashSet<Kind>, HashSet<&'static str>) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut engine = Engine::new(seed_set()).unwrap();
    // The set and its identity just before every change that made a step, oldest first. A fold
    // or expand between two steps belongs to the later step's snapshot.
    let mut states: Vec<(Project, u64)> = Vec::new();
    let mut applied = HashSet::new();
    let mut transport = HashSet::new();
    let mut attempts = 0;
    while states.len() < steps && attempts < steps * 40 {
        attempts += 1;
        let before = engine.project().clone();
        let before_id = engine.content_id();
        let (undo_label, redo_label) = (engine.undo_label(), engine.redo_label());
        let (kind, result) = if rng.below(8) == 0 {
            let (name, command) = transport_command(&mut rng);
            transport.insert(name);
            (None, engine.execute(command))
        } else {
            let kind = ALL[rng.below(ALL.len())];
            if is_view_state(kind) && !with_view_state {
                continue;
            }
            let Some(edit) = generate(kind, engine.project(), &mut rng) else {
                continue;
            };
            assert_eq!(kind_of(&edit), kind);
            (Some(kind), engine.execute(Command::Edit(edit)))
        };
        match result {
            Ok(_) => {}
            // Only a refused edit is a rejection; any other error is the engine or scheduler failing.
            Err(EngineError::Project(_)) => {
                assert_eq!(engine.project(), &before, "a rejected edit changed the set");
                continue;
            }
            Err(error) => panic!("seed {seed}: {error}"),
        }
        if let Some(kind) = kind {
            applied.insert(kind);
        }
        if kind.is_some_and(is_mapping) {
            assert_eq!(
                (engine.undo_label(), engine.redo_label()),
                (undo_label, redo_label),
                "a mapping edit made or changed a step"
            );
        } else if !kind.is_some_and(is_view_state) && engine.project() != &before {
            states.push((before, before_id));
        }
    }
    let steps_made = states.len();
    println!("seed {seed}: {steps_made} steps, {} kinds", applied.len());
    // Each undo and redo must land exactly on the snapshot with the current fold and expand
    // carried over, and on its recorded identity unless the carry changed the snapshot.
    let check = |engine: &Engine, current: &Project, snapshot: &(Project, u64), what: &str| {
        let expected = carried(current, &snapshot.0);
        assert_eq!(
            engine.project(),
            &expected,
            "{what} landed on the wrong set"
        );
        if expected == snapshot.0 {
            assert_eq!(
                engine.content_id(),
                snapshot.1,
                "{what} restores the identity"
            );
        } else {
            assert_ne!(
                engine.content_id(),
                snapshot.1,
                "{what} after a carry is new content"
            );
        }
    };
    let mut taken_back = Vec::new();
    for earlier in states.iter().rev() {
        let current = (engine.project().clone(), engine.content_id());
        engine.execute(Command::Undo).unwrap();
        check(&engine, &current.0, earlier, "undo");
        taken_back.push(current);
    }
    assert!(
        !engine.can_undo(),
        "{steps_made} steps undone, none left over"
    );
    while let Some(later) = taken_back.pop() {
        let current = engine.project().clone();
        engine.execute(Command::Redo).unwrap();
        check(&engine, &current, &later, "redo");
    }
    assert!(!engine.can_redo());
    // A value-only edit after all of it: a scheduler that drifted from the engine's set
    // (a stale tempo, say) stops on the first one.
    let set_volume = |engine: &mut Engine, value: f64| {
        let main = engine.project().main.id;
        engine
            .execute(Command::Edit(ProjectEdit::SetParameter {
                target: ParameterRef::volume(main),
                values: vec![(FieldKey("value".into()), value)],
            }))
            .unwrap();
    };
    set_volume(&mut engine, 0.25);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(
        engine.snapshot().status.scheduler_failure,
        None,
        "seed {seed}"
    );
    set_volume(&mut engine, 0.75);
    (applied, transport)
}

#[test]
fn undo_all_restores_the_start_and_redo_all_the_end_across_every_edit_kind() {
    let mut applied = HashSet::new();
    let mut transport = HashSet::new();
    for seed in 1..=24 {
        let (kinds, commands) = walk(seed, 150, seed % 2 == 0);
        applied.extend(kinds);
        transport.extend(commands);
    }
    assert_eq!(transport.len(), 3, "{transport:?}");
    let missing: Vec<_> = ALL.iter().filter(|k| !applied.contains(k)).collect();
    assert!(
        missing.is_empty(),
        "no valid edit was generated for {missing:?}; teach `generate` a case"
    );
}
