//! Contextual help is GUI state only; hovering never edits the set or commits a field.
//!
//! The protocol (ids, hover, wrappers) lives in the control layer, `control::help`; this
//! panel supplies the app's help ids (`registry::Id`) and subjects, and renders what hovers
//! report.
mod registry;

pub(crate) use crate::control::help::{Help, region, region_declaring, widget};
use crate::control::help::{HelpId, Source};
use crate::messages::Message;
use crate::style::detail::{HEADER_SIZE, INFO_VIEW_WIDTH, TEXT_SIZE};
use iced::widget::{column, container, scrollable, text};
use iced::{Element, Length};
use qtable_engine::model::{
    DeviceId, DeviceRef, EntryKey, EventRef, FieldKey, FieldRef, OscAddress, ParameterKey,
    ParameterRef, PathLevel, Project, SceneId, TrackId,
};
pub(crate) use registry::Id;

pub(crate) type Hover = crate::control::help::Hover<Id>;

impl HelpId for Id {
    type Subject = Subject;
}

/// A Target entry by its stable keys, never by its place in the events or parameters list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntryRef {
    Parameter(ParameterRef),
    Event { track: TrackId, event: EventRef },
}

impl EntryRef {
    /// The entry of a Target device that `key` names, as a parameter or as an event.
    pub(crate) fn target(
        parameter: bool,
        track: TrackId,
        device: DeviceId,
        key: &EntryKey,
    ) -> Self {
        if parameter {
            Self::Parameter(ParameterRef {
                track,
                device: DeviceRef::Chain(device),
                key: ParameterKey(key.0.clone()),
            })
        } else {
            Self::Event {
                track,
                event: EventRef {
                    device,
                    key: key.clone(),
                },
            }
        }
    }

    pub(crate) fn track(&self) -> TrackId {
        match self {
            Self::Parameter(parameter) => parameter.track,
            Self::Event { track, .. } => *track,
        }
    }

    fn device(&self) -> Option<DeviceId> {
        match self {
            Self::Parameter(ParameterRef {
                device: DeviceRef::Chain(device),
                ..
            }) => Some(*device),
            Self::Parameter(_) => None,
            Self::Event { event, .. } => Some(event.device),
        }
    }

    fn is_parameter(&self) -> bool {
        matches!(self, Self::Parameter(_))
    }

    fn key(&self) -> &str {
        match self {
            Self::Parameter(parameter) => &parameter.key.0,
            Self::Event { event, .. } => &event.key.0,
        }
    }
}

/// What a path field stands for, so the Info View can list its addresses while the pointer
/// rests on it. The addresses are worked out when drawn, never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subject {
    /// A track's own Path.
    Path(TrackId),
    /// A track's Volume Path.
    Volume(TrackId),
    /// A clip lane, by the clip's track and scene and the lane's event, so a hover that
    /// outlives a change to the lane list resolves to nothing rather than to another lane.
    /// Only a capture builds one without a pointer (`dev-hooks`).
    #[cfg_attr(not(any(test, feature = "dev-hooks")), allow(dead_code))]
    Lane {
        track: TrackId,
        scene: SceneId,
        event: EventRef,
    },
    /// A Target's own Path.
    TargetPath(TrackId, DeviceId),
    /// An argument of a Target entry, by the entry and the argument's key.
    TargetField(EntryRef, FieldKey),
    /// An entry of a Target.
    TargetEntry(EntryRef),
}

impl Subject {
    /// The argument a parameter-field subject stands for.
    pub(crate) fn parameter_field(&self) -> Option<FieldRef> {
        match self {
            Self::TargetField(EntryRef::Parameter(parameter), field) => Some(FieldRef {
                parameter: parameter.clone(),
                field: field.clone(),
            }),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    current: Option<(Source, Id, Option<Subject>)>,
    /// Capture-only: window focus changes cannot clear a seeded entry; a hover can replace it.
    pinned: bool,
}

impl State {
    pub(crate) fn update(&mut self, hover: Hover) {
        if self.pinned && !matches!(hover, Hover::Enter(..)) {
            return;
        }
        match hover {
            Hover::Enter(source, id, subject) => {
                self.pinned = false;
                self.current = Some((source, id, subject));
            }
            Hover::Leave(source)
                if self
                    .current
                    .as_ref()
                    .is_some_and(|(owner, ..)| *owner == source) =>
            {
                self.current = None;
            }
            Hover::Clear => self.current = None,
            Hover::Leave(_) => {}
        }
    }

    pub(crate) fn id(&self) -> Id {
        self.current.as_ref().map_or(Id::InfoView, |(_, id, _)| *id)
    }

    pub(crate) fn subject(&self) -> Option<&Subject> {
        self.current
            .as_ref()
            .and_then(|(.., subject)| subject.as_ref())
    }

    /// Capture-only: start with a subject's help showing, without a pointer.
    #[cfg(any(test, feature = "dev-hooks"))]
    pub(crate) fn show(&mut self, id: Id, subject: Subject) {
        self.current = Some((Source::new(), id, Some(subject)));
        self.pinned = true;
    }
}

const NO_DESTINATION: &str = "No destination configured.";

struct AddressDetail {
    address: String,
    breakdown: String,
}

fn path_subject(
    project: &Project,
    subject: &Subject,
) -> Option<(
    TrackId,
    PathLevel,
    crate::control::path_field::Segment,
    bool,
)> {
    use crate::control::path_field::{Ownership, Segment};
    let piece = |label, path: &str| Segment::new(label, path, Ownership::Own);
    match subject {
        Subject::TargetField(entry, _) | Subject::TargetEntry(entry) => {
            let device = entry.device()?;
            let (_, target) = target_of(project, entry.track(), device)?;
            let path = if entry.is_parameter() {
                &target
                    .parameters
                    .iter()
                    .find(|p| p.key.0 == entry.key())?
                    .path
            } else {
                &target.events.iter().find(|e| e.key.0 == entry.key())?.path
            };
            let label = if entry.is_parameter() {
                "Param"
            } else {
                "Event"
            };
            let (OscAddress::Absolute(text) | OscAddress::Relative(text)) = path;
            Some((
                entry.track(),
                PathLevel::Device(device),
                piece(label, text),
                matches!(path, OscAddress::Absolute(_)),
            ))
        }
        Subject::TargetPath(track, device) => {
            let (_, target) = target_of(project, *track, *device)?;
            Some((
                *track,
                PathLevel::Under,
                piece("Target", target.path.as_str()),
                false,
            ))
        }
        Subject::Path(track) => {
            let own = project.track(*track)?;
            let label = if own.capabilities().destination {
                "Output"
            } else if own.capabilities().foldable {
                "Group"
            } else {
                "Track"
            };
            Some((
                *track,
                PathLevel::Track,
                piece(label, own.path.as_str()),
                false,
            ))
        }
        Subject::Volume(track) => Some((
            *track,
            PathLevel::Under,
            piece(
                "Volume",
                &project.track(*track)?.mixer_device.volume.output.address,
            ),
            false,
        )),
        Subject::Lane {
            track,
            scene,
            event,
        } => {
            let owner = project.track(*track)?;
            let reference = lane_event(project, *track, *scene, event)?;
            let path = &owner.event_def(&reference)?.1.path;
            let (OscAddress::Absolute(text) | OscAddress::Relative(text)) = path;
            Some((
                *track,
                PathLevel::Device(reference.device),
                piece("Event", text),
                matches!(path, OscAddress::Absolute(_)),
            ))
        }
    }
}

fn address_details(project: &Project, subject: &Subject) -> Option<Vec<AddressDetail>> {
    let (source, level, piece, absolute) = path_subject(project, subject)?;
    let breakdown = |segments: &[crate::control::path_field::Segment]| {
        segments
            .iter()
            .filter(|s| !s.path.is_empty())
            .map(|s| format!("{} {}", s.label, s.path))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    if absolute {
        return Some(vec![AddressDetail {
            address: piece.path.clone(),
            breakdown: format!("{} (Absolute)", breakdown(&[piece])),
        }]);
    }
    let mut lines: Vec<_> = crate::path_context::paths(project, source, level)
        .into_iter()
        .map(|(destination, mut segments)| {
            segments.push(piece.clone());
            let address: String = segments.iter().map(|s| s.path.as_str()).collect();
            let off = if destination.mixer.active {
                ""
            } else {
                " (off)"
            };
            AddressDetail {
                address: format!("{}: {address}{off}", destination.name),
                breakdown: breakdown(&segments),
            }
        })
        .collect();
    if lines.is_empty() {
        lines.push(AddressDetail {
            address: NO_DESTINATION.to_owned(),
            breakdown: String::new(),
        });
    }
    Some(lines)
}

#[cfg(test)]
pub(crate) fn addresses(project: &Project, subject: &Subject) -> Option<Vec<String>> {
    Some(
        address_details(project, subject)?
            .into_iter()
            .map(|line| line.address)
            .collect(),
    )
}

fn target_of(
    project: &Project,
    track: TrackId,
    device: qtable_engine::model::DeviceId,
) -> Option<(
    &qtable_engine::model::Device,
    &qtable_engine::model::TargetDevice,
)> {
    let owner = project.track(track)?;
    let found = owner.device(device)?;
    Some((found, found.target()?))
}

/// How many lanes in how many clips play an event of the Target; `None` for other subjects
/// and for parameters. This count has its one home here (device-chain design, Info View).
pub(crate) fn uses(project: &Project, subject: &Subject) -> Option<String> {
    let Subject::TargetEntry(EntryRef::Event { track, event }) = subject else {
        return None;
    };
    let (lanes, clips) = project.track(*track)?.event_uses(event);
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    Some(format!(
        "Used by {} in {}",
        plural(lanes, "lane", "lanes"),
        plural(clips, "clip", "clips")
    ))
}

/// The lane's event, if the clip on that track and scene still has a lane for it.
fn lane_event(
    project: &Project,
    track: TrackId,
    scene: SceneId,
    event: &EventRef,
) -> Option<EventRef> {
    let slot = project.scenes.iter().position(|s| s.id == scene)?;
    let clip = project.track(track)?.clip(slot)?;
    clip.lane_refs().into_iter().find(|r| r == event)
}

/// A lane's event, as `Track::event_label` tells it apart; `None` for other subjects.
pub(crate) fn label(project: &Project, subject: &Subject) -> Option<String> {
    let Subject::Lane {
        track,
        scene,
        event,
    } = subject
    else {
        return None;
    };
    let owner = project.track(*track)?;
    let reference = lane_event(project, *track, *scene, event)?;
    owner.event_def(&reference)?;
    Some(owner.event_label(&reference).into_owned())
}

fn field_label(project: &Project, subject: &Subject) -> Option<String> {
    let Subject::TargetField(entry, key) = subject else {
        return None;
    };
    let (_, target) = target_of(project, entry.track(), entry.device()?)?;
    let (name, fields) = if entry.is_parameter() {
        let p = target.parameters.iter().find(|p| p.key.0 == entry.key())?;
        (p.display_name(), &p.fields)
    } else {
        let e = target.events.iter().find(|e| e.key.0 == entry.key())?;
        (e.display_name(), &e.fields)
    };
    let field = fields.iter().position(|f| &f.key == key)?;
    Some(format!(
        "{name} · {} ({} of {})",
        fields[field].display_name(field),
        field + 1,
        fields.len()
    ))
}

/// One-line registration for controls in the application's message domain.
pub(crate) fn help<'a>(content: impl Into<Element<'a, Message>>, id: Id) -> Element<'a, Message> {
    widget(content, id, Message::Info)
}

pub(crate) fn view(state: &State, project: &Project) -> Element<'static, Message> {
    let entry = state.id().entry();
    let mut body = column![
        text(entry.title).size(HEADER_SIZE),
        text(entry.text).size(TEXT_SIZE)
    ]
    .spacing(6);
    if let Some(label) = state.subject().and_then(|s| label(project, s)) {
        body = body.push(text("Event").size(TEXT_SIZE).color(crate::style::DISABLED));
        body = body.push(text(label).size(TEXT_SIZE));
    }
    if let Some(name) = state.subject().and_then(|s| field_label(project, s)) {
        body = body.push(text(name).size(TEXT_SIZE));
    }
    if let Some(lines) = state.subject().and_then(|s| address_details(project, s)) {
        body = body.push(
            text("Addresses")
                .size(TEXT_SIZE)
                .color(crate::style::DISABLED),
        );
        for line in lines {
            body = body.push(text(line.address).size(TEXT_SIZE));
            if !line.breakdown.is_empty() {
                body = body.push(text(line.breakdown).size(TEXT_SIZE));
            }
        }
    }
    if let Some(used) = state.subject().and_then(|s| uses(project, s)) {
        body = body.push(text(used).size(TEXT_SIZE));
    }
    container(scrollable(body))
        .padding(8)
        .width(Length::Fixed(INFO_VIEW_WIDTH))
        .height(Length::Fill)
        .style(crate::style::panel)
        .into()
}

#[cfg(test)]
mod tests;
