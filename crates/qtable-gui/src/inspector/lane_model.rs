//! The one lane presentation model: every lane of a clip, event or envelope, as a row with a
//! height, a header and (for envelopes) its curve. Both clip editor views and the lane traits
//! of `event-lanes-like-tracks` read this; nothing here draws or edits (`polyline` only shapes the path a view strokes).
//!
//! Event rows follow device/definition order, then the clip's envelopes in stored order.
//! Generated rows need no clip lane; selection and edits keep their stable EventRef.
use super::event_timeline_math::{ENVELOPE_LANE_HEIGHT, EVENT_LANE_HEIGHT};
use super::point_gesture::{Lane as PointLane, Plot};
use crate::parameter_gesture::{AutomationState, Effective, effective_in};
use iced::Point;
use iced::widget::canvas::Path;
use qtable_engine::model::{
    ArgumentRef, Axis, BeatTime, Breakpoint, Clip, DeviceRef, EventRef, FieldRef, Project, TrackId,
    format_float,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RowKind {
    /// A generated event row; `index` adapts to `Clip::lane_refs` when present.
    Event {
        index: Option<usize>,
        reference: EventRef,
    },
    Envelope(std::rc::Rc<EnvelopeRow>),
}

/// What an envelope lane needs to draw and label itself, resolved once per model build.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EnvelopeRow {
    pub target: ArgumentRef,
    pub field: FieldRef,
    pub axis: Axis,
    pub min: f64,
    pub max: f64,
    /// Enum option labels, or Off and On for a Toggle; empty for numbers.
    pub labels: Vec<String>,
    /// The argument's control on its Target; `None` for Mixer Volume, which is a Float.
    pub control: Option<qtable_engine::model::Control>,
    pub points: Vec<Breakpoint>,
    /// The argument's output Threshold; Draw Mode's freehand thinning tolerates this much.
    pub threshold: f64,
    /// The lane has its own loop region, so the overview shows it read-only.
    pub region: Option<qtable_engine::model::region::Region>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LaneRow {
    /// Index in the complete mixed lane list, retained by filtered views.
    pub source: usize,
    pub kind: RowKind,
    /// The device in parent ink; empty when it no longer resolves.
    pub device: String,
    /// The event or argument name.
    pub name: String,
    /// Top edge, from the first lane's top.
    pub top: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LaneModel {
    pub rows: Vec<LaneRow>,
}

/// A single selected clip's immutable presentation, shared by both canvas views.
/// Live values, selection and playhead deliberately stay outside this revision cache.
#[derive(Default)]
pub(crate) struct ModelCache(std::cell::RefCell<Option<CachedModel>>);

struct CachedModel {
    key: (super::event_editor::ClipKey, u64),
    clip: std::rc::Rc<Clip>,
    model: std::rc::Rc<LaneModel>,
    events: std::rc::Rc<LaneModel>,
}

impl ModelCache {
    pub(crate) fn get(
        &self,
        project: &Project,
        key: super::event_editor::ClipKey,
        revision: u64,
        clip: &Clip,
    ) -> (std::rc::Rc<Clip>, std::rc::Rc<LaneModel>) {
        let mut cached = self.0.borrow_mut();
        if cached.as_ref().is_none_or(|c| c.key != (key, revision)) {
            let model = LaneModel::build(project, key.track, clip);
            let events = std::rc::Rc::new(model.events());
            *cached = Some(CachedModel {
                key: (key, revision),
                clip: std::rc::Rc::new(clip.clone()),
                model: std::rc::Rc::new(model),
                events,
            });
        }
        let cached = cached.as_ref().expect("model populated above");
        (cached.clip.clone(), cached.model.clone())
    }

    /// The event-only projection shares the same clip/revision cache as Overview.
    pub(crate) fn events(&self) -> std::rc::Rc<LaneModel> {
        self.0
            .borrow()
            .as_ref()
            .expect("get populates the model")
            .events
            .clone()
    }
}

impl LaneModel {
    pub(crate) fn build(project: &Project, track: TrackId, clip: &Clip) -> Self {
        let owner = project.track(track);
        let refs = clip.lane_refs();
        let mut rows = Vec::new();
        let mut top = 0.0;
        let mut push = |kind: RowKind, device: String, name: String, height: f32| {
            rows.push(LaneRow {
                source: rows.len(),
                kind,
                device,
                name,
                top,
                height,
            });
            top += height;
        };
        for identity in super::event_editor::lanes::displayed(owner, clip) {
            match identity {
                super::event_editor::lanes::Identity::Event(reference) => {
                    let (device, name) = owner.map_or_else(Default::default, |t| {
                        (
                            t.event_def(&reference)
                                .map(|(d, _)| d.name.clone())
                                .unwrap_or_default(),
                            t.event_label(&reference).into_owned(),
                        )
                    });
                    push(
                        RowKind::Event {
                            index: refs.iter().position(|r| *r == reference),
                            reference,
                        },
                        device,
                        name,
                        EVENT_LANE_HEIGHT,
                    );
                }
                super::event_editor::lanes::Identity::Envelope(target) => {
                    let envelope = clip
                        .lanes
                        .iter()
                        .filter_map(|l| l.envelope())
                        .find(|e| e.target == target)
                        .expect("displayed envelope belongs to clip");
                    let (device, name) =
                        owner.map_or_else(Default::default, |t| argument_names(t, &target));
                    push(
                        RowKind::Envelope(std::rc::Rc::new(envelope_row(owner, track, envelope))),
                        device,
                        name,
                        ENVELOPE_LANE_HEIGHT,
                    );
                }
            }
        }
        Self { rows }
    }

    /// Events keeps event identities and order, closing the gaps left by envelope rows.
    pub(crate) fn events(&self) -> Self {
        let mut top = 0.0;
        let rows = self
            .rows
            .iter()
            .filter(|r| matches!(r.kind, RowKind::Event { .. }))
            .map(|r| {
                let mut row = r.clone();
                row.top = top;
                top += row.height;
                row
            })
            .collect();
        Self { rows }
    }

    /// A model of plain event lanes, for tests that need rows but no project.
    #[cfg(test)]
    pub(crate) fn of_events(references: &[EventRef]) -> Self {
        Self {
            rows: references
                .iter()
                .enumerate()
                .map(|(index, reference)| LaneRow {
                    source: index,
                    kind: RowKind::Event {
                        index: Some(index),
                        reference: reference.clone(),
                    },
                    device: String::new(),
                    name: String::new(),
                    top: index as f32 * EVENT_LANE_HEIGHT,
                    height: EVENT_LANE_HEIGHT,
                })
                .collect(),
        }
    }

    pub(crate) fn height(&self) -> f32 {
        self.rows.last().map_or(0.0, |r| r.top + r.height)
    }

    /// The row covering `y`, counted from the first lane's top.
    pub(crate) fn row_at(&self, y: f32) -> Option<usize> {
        if y < 0.0 {
            return None;
        }
        let row = self.rows.partition_point(|r| r.top + r.height <= y);
        (row < self.rows.len()).then_some(row)
    }

    /// The row of the event lane that plays `reference`.
    pub(crate) fn event_row(&self, reference: &EventRef) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| matches!(&r.kind, RowKind::Event { reference: r, .. } if r == reference))
    }
}

fn envelope_row(
    owner: Option<&qtable_engine::model::Track>,
    track: TrackId,
    envelope: &qtable_engine::model::AutomationEnvelope,
) -> EnvelopeRow {
    let spec = owner.and_then(|t| t.argument_spec(&envelope.target));
    let axis = owner
        .and_then(|t| t.argument_axis(&envelope.target))
        .unwrap_or(Axis::Continuous);
    EnvelopeRow {
        field: envelope.target.on(track),
        target: envelope.target.clone(),
        axis,
        min: spec.as_ref().map_or(0.0, |s| s.min),
        max: spec.as_ref().map_or(1.0, |s| s.max),
        labels: owner
            .map(|t| value_labels(t, &envelope.target))
            .unwrap_or_default(),
        control: owner
            .and_then(|t| argument_field(t, &envelope.target))
            .map(|f| f.control),
        points: envelope.points.clone(),
        threshold: owner
            .and_then(|t| argument_field(t, &envelope.target))
            .map_or(0.0, |f| f.effective_threshold()),
        region: envelope.region,
    }
}

fn definition<'a>(
    track: &'a qtable_engine::model::Track,
    target: &ArgumentRef,
) -> Option<(
    &'a qtable_engine::model::Device,
    &'a qtable_engine::model::ParameterDef,
    usize,
)> {
    let (device, parameter) = track.argument_parameter(target)?;
    let position = parameter
        .fields
        .iter()
        .position(|f| f.key == target.field)?;
    Some((device, parameter, position))
}

/// The definition behind an argument of a Target device; the Mixer's Volume has none.
pub(crate) fn argument_field<'a>(
    track: &'a qtable_engine::model::Track,
    target: &ArgumentRef,
) -> Option<&'a qtable_engine::model::FieldDef> {
    let (_, parameter, position) = definition(track, target)?;
    parameter.fields.get(position)
}

/// The device and argument names a lane header shows. A multi-argument parameter reads
/// "Position · x", as the chooser will list it.
pub(crate) fn argument_names(
    track: &qtable_engine::model::Track,
    target: &ArgumentRef,
) -> (String, String) {
    match target.device {
        DeviceRef::Mixer => ("Mixer".into(), "Volume".into()),
        DeviceRef::Chain(_) => definition(track, target).map_or_else(
            Default::default,
            |(device, parameter, position)| {
                let name = if parameter.fields.len() == 1 {
                    parameter.display_name().to_owned()
                } else {
                    format!(
                        "{} · {}",
                        parameter.display_name(),
                        parameter.fields[position].display_name(position)
                    )
                };
                (device.name.clone(), name)
            },
        ),
    }
}

fn value_labels(track: &qtable_engine::model::Track, target: &ArgumentRef) -> Vec<String> {
    let Some((_, parameter, position)) = definition(track, target) else {
        return vec![];
    };
    let field = &parameter.fields[position];
    match field.control {
        qtable_engine::model::Control::Toggle => vec!["Off".into(), "On".into()],
        qtable_engine::model::Control::Enum => {
            field.options.iter().map(|o| o.label.clone()).collect()
        }
        _ => vec![],
    }
}

impl EnvelopeRow {
    /// The row of an argument the clip has no lane for (yet): no points, same axis and range.
    pub(crate) fn of_argument(project: &Project, track: TrackId, target: &ArgumentRef) -> Self {
        envelope_row(
            project.track(track),
            track,
            &qtable_engine::model::AutomationEnvelope {
                region: None,
                target: target.clone(),
                points: vec![],
            },
        )
    }

    /// A value in the argument's control form: a number, an Enum label, or Off and On.
    pub(crate) fn value_text(&self, value: f64) -> String {
        match self.axis {
            Axis::Discrete => self
                .labels
                .get(value.round().max(0.0) as usize)
                .cloned()
                .unwrap_or_else(|| format_float(value, Some(2))),
            Axis::Stepped => format!("{}", value.round() as i64),
            Axis::Continuous => format_float(value, Some(2)),
        }
    }

    /// This row as the gestures and every drawing see it: `Lane` is the one x/y mapper.
    pub(crate) fn lane(&self, plot: Plot, length: BeatTime, grid: u64) -> PointLane {
        PointLane::new(
            plot,
            (length, self.axis, self.min, self.max, grid),
            &self.target,
            &self.points,
        )
    }

    /// The curve as `(ticks, value)` samples from the engine's own evaluation
    /// (`AutomationEnvelope::value_at`): one per `step` ticks, plus each breakpoint and the
    /// instant before it, so jumps and steps come out vertical. Stepped (Int) arguments draw
    /// as the unrounded line, as design section 2 says; only Discrete ones hold.
    pub(crate) fn samples(&self, length: BeatTime, step: f64) -> Vec<(f64, f64)> {
        self.samples_as(length, step, self.line_axis())
    }

    /// The axis the line itself draws on: Stepped (Int) arguments draw unrounded.
    fn line_axis(&self) -> Axis {
        match self.axis {
            Axis::Stepped => Axis::Continuous,
            axis => axis,
        }
    }

    /// `samples` evaluated on `axis`: the one-lane view draws a Stepped argument's rounded
    /// staircase faintly behind its line by asking for `Axis::Stepped`.
    pub(crate) fn samples_as(&self, length: BeatTime, step: f64, axis: Axis) -> Vec<(f64, f64)> {
        self.samples_within((0.0, length.ticks() as f64), step, axis)
    }

    /// `samples_as` over the ticks `[window.0, window.1]` only: one sample every `step` ticks
    /// from the window's start, its two ends, and each breakpoint inside it with the instant
    /// before. A zoomed view asks for its visible window, so the work follows the pixels it
    /// draws, not the clip's length.
    pub(crate) fn samples_within(
        &self,
        window: (f64, f64),
        step: f64,
        axis: Axis,
    ) -> Vec<(f64, f64)> {
        let envelope = qtable_engine::model::AutomationEnvelope {
            region: None,
            target: self.target.clone(),
            points: self.points.clone(),
        };
        let (start, end) = window;
        let mut at = vec![start, end];
        let step = step.max(1.0);
        let mut tick = start + step;
        while tick < end {
            at.push(tick);
            tick += step;
        }
        for p in &self.points {
            let t = p.time.ticks() as f64;
            if t < start || t > end {
                continue;
            }
            at.push(t);
            if t > 0.0 {
                at.push(t - 0.001);
            }
        }
        at.sort_by(f64::total_cmp);
        at.dedup();
        at.into_iter()
            .filter_map(|t| envelope.value_at(t, axis).map(|v| (t, v)))
            .collect()
    }

    /// `points` as one polyline on `lane`, evaluated on `axis` (`None` for the line itself, as
    /// `samples` gives it; one sample a pixel, plus each
    /// breakpoint) and kept to `span` ticks. The one stroke every view of a curve shares, so a
    /// look change lands once.
    pub(crate) fn polyline(
        &self,
        points: &[Breakpoint],
        lane: &PointLane,
        axis: Option<Axis>,
        span: (f64, f64),
    ) -> Path {
        let per_pixel = lane.time.ticks_per_px();
        let shown = Self {
            points: points.to_vec(),
            ..self.clone()
        };
        // The line itself draws Stepped arguments unrounded (`samples`); a given axis is taken
        // as it comes.
        let axis = axis.unwrap_or_else(|| self.line_axis());
        let samples = shown.samples_within(lane.time.visible_range(), per_pixel, axis);
        Path::new(|p| {
            let mut first = true;
            for (ticks, value) in samples
                .into_iter()
                .filter(|(ticks, _)| *ticks >= span.0 && *ticks <= span.1)
            {
                let at = Point::new(lane.x(BeatTime(ticks.round() as u64)), lane.y(value));
                if first {
                    p.move_to(at);
                    first = false;
                } else {
                    p.line_to(at);
                }
            }
        })
    }
}

impl LaneModel {
    /// One entry per row: `None` for an event lane. An argument the engine isn't driving shows
    /// its stored value, following (a clip that isn't playing has overridden nothing).
    pub(crate) fn live(
        &self,
        project: &Project,
        automation: &[qtable_engine::scheduler::AutomationValue],
    ) -> Vec<Option<Effective>> {
        self.rows
            .iter()
            .map(|row| {
                let RowKind::Envelope(envelope) = &row.kind else {
                    return None;
                };
                live_value(project, automation, &envelope.field)
            })
            .collect()
    }
}

/// What an envelope header shows live: an argument's value as the engine reports it, else as
/// the project stores it, following (a clip that isn't playing has overridden nothing).
pub(crate) fn live_value(
    project: &Project,
    automation: &[qtable_engine::scheduler::AutomationValue],
    field: &FieldRef,
) -> Option<Effective> {
    effective_in(automation, field).or_else(|| {
        project.field(field).map(|f| Effective {
            value: f.value,
            state: AutomationState::None,
        })
    })
}

mod region;

#[cfg(test)]
pub(crate) mod tests;
