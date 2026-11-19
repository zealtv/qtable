//! Snapshot-only output load and the shared Output/Main Throughput section.
use crate::app::App;
use crate::control::{LoadMeter, Readout};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId};
use crate::info::{Help, Id};
use crate::messages::Message;
use crate::style::detail;
use iced::widget::{Space, column, container, row, text};
use iced::{Element, Length};
use qtable_engine::model::TrackId;
use qtable_engine::scheduler::Throughput;

pub(crate) const STRIP_HEIGHT: f32 = 22.0;

pub(crate) fn strip(app: &App, id: TrackId) -> Element<'_, Message> {
    container(load(app, id))
        .padding(detail::STRIP_INSET)
        .width(Length::Fill)
        .into()
}

fn value(app: &App, id: TrackId) -> Throughput {
    if app
        .project()
        .track(id)
        .and_then(|t| t.output_stage())
        .is_none()
    {
        return Throughput::default();
    }
    app.snapshot
        .output_throughput
        .iter()
        .find(|(held, _)| *held == id)
        .map_or(Throughput::default(), |(_, value)| *value)
}

fn fraction(app: &App, id: TrackId, messages: u64) -> f64 {
    let budget = app
        .project()
        .track(id)
        .and_then(|t| t.output_stage())
        .and_then(|s| s.budget());
    let scale = budget.map_or_else(
        || {
            app.snapshot
                .output_throughput
                .iter()
                .filter(|(id, _)| {
                    app.project()
                        .track(*id)
                        .and_then(|t| t.output_stage())
                        .is_some()
                })
                .map(|(_, value)| value.messages)
                .max()
                .unwrap_or(0) as f64
        },
        |budget| budget as f64,
    );
    if scale == 0.0 {
        0.0
    } else {
        (messages as f64 / scale).min(1.0)
    }
}

pub(crate) fn load(app: &App, id: TrackId) -> Element<'_, Message> {
    if !app
        .project()
        .track_capabilities(id)
        .is_some_and(|c| c.destination)
    {
        return Space::new().height(14).into();
    }
    let value = value(app, id);
    let enabled = app
        .project()
        .track(id)
        .and_then(|t| t.output_stage())
        .is_some();
    let theme = app.theme;
    let rate = if value.messages < 10_000 {
        format!("{}/s", value.messages)
    } else {
        format!("{:.1}k/s", value.messages as f64 / 1000.0)
    };
    row![
        app.controls().canvas(
            Control::new(
                ControlId::action(View::Mixer, "output-load", ControlOwner::Track(id)),
                Id::OutputLoad,
                LoadMeter {
                    fraction: fraction(app, id, value.messages),
                    deferring: value.deferring
                },
            )
            .enabled(enabled),
            None
        ),
        text(rate)
            .size(detail::COMPACT_TEXT_SIZE)
            .color(if !enabled {
                theme.text_disabled
            } else if value.deferring {
                theme.chosen_default
            } else {
                theme.control_foreground
            })
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .wrapping(iced::widget::text::Wrapping::None),
    ]
    .spacing(2)
    .height(14)
    .align_y(iced::alignment::Vertical::Center)
    .info(Id::OutputLoad, Message::Info)
}

pub(crate) fn view(app: &App, id: TrackId) -> Element<'_, Message> {
    let value = value(app, id);
    let theme = app.theme;
    let view = if id == app.project().main.id {
        View::Main
    } else {
        View::Output
    };
    let readout = |label: &'static str, number: String, help| {
        column![
            text(label)
                .size(detail::TEXT_SIZE)
                .color(theme.text_disabled),
            app.controls().readout_box(
                Control::new(
                    ControlId::action(view, label, ControlOwner::Track(id)),
                    help,
                    Readout::right(),
                ),
                number,
            ),
        ]
        .spacing(detail::LABEL_GAP)
        .width(Length::Fill)
        .info(help, Message::Info)
    };
    column![
        detail::pair(
            readout(
                "Messages/s",
                value.messages.to_string(),
                Id::ThroughputMessages
            ),
            readout(
                "Packets/s",
                value.packets.to_string(),
                Id::ThroughputPackets
            )
        ),
        detail::pair(
            readout(
                "Bytes/s",
                format!("{:.1} kB", value.bytes as f64 / 1000.0),
                Id::ThroughputBytes
            ),
            readout("Merged/s", value.merged.to_string(), Id::ThroughputMerged)
        ),
        detail::pair(
            readout(
                "Deferred/s",
                value.deferred.to_string(),
                Id::ThroughputDeferred
            ),
            readout(
                "Skipped/s",
                value.skipped.to_string(),
                Id::ThroughputSkipped
            )
        ),
        detail::pair(
            readout(
                "Oversize",
                value.oversize.to_string(),
                Id::ThroughputOversize
            ),
            readout(
                "Frame late",
                value.frames_late.to_string(),
                Id::ThroughputFramesLate
            )
        ),
    ]
    .spacing(detail::CONTROL_GAP)
    .into()
}

/// Unattended screenshots can hold a busy/deferring snapshot without editing the set.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App) {
    if std::env::var_os("QTABLE_CAPTURE_THROUGHPUT").is_none() {
        return;
    }
    app.snapshot.output_throughput = app
        .project()
        .outputs
        .iter()
        .chain(std::iter::once(&app.project().main))
        .filter(|track| track.output_stage().is_some())
        .enumerate()
        .map(|(index, track)| {
            (
                track.id,
                if index == 0 {
                    Throughput {
                        messages: 600,
                        packets: 30,
                        bytes: 21400,
                        merged: 1204,
                        deferred: 112,
                        skipped: 7,
                        oversize: 1,
                        frames_late: 2,
                        deferring: true,
                    }
                } else {
                    Throughput {
                        messages: 42,
                        packets: 42,
                        bytes: 1680,
                        ..Throughput::default()
                    }
                },
            )
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::engine::Engine;
    use qtable_engine::model::Project;

    fn app() -> App {
        let project = qtable_engine::file::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/paths.ron"
        ))
        .unwrap();
        App::new(Engine::new(project).unwrap(), String::new(), None)
    }

    #[test]
    fn load_compares_destinations_or_budget_and_unset_main_ignores_stale_counts() {
        let mut app = app();
        let a = app.project().outputs[0].id;
        let b = app.project().outputs[1].id;
        let main = app.project().main.id;
        app.snapshot.output_throughput = vec![
            (
                a,
                Throughput {
                    messages: 200,
                    ..Throughput::default()
                },
            ),
            (
                b,
                Throughput {
                    messages: 100,
                    ..Throughput::default()
                },
            ),
            (
                main,
                Throughput {
                    messages: 400,
                    ..Throughput::default()
                },
            ),
        ];
        assert_eq!(fraction(&app, a, 200), 0.5);
        assert_eq!(fraction(&app, b, 100), 0.25);
        let mut stage = app
            .project()
            .output(a)
            .unwrap()
            .output_stage()
            .unwrap()
            .clone();
        stage.budget_on = true;
        stage.budget = 1000;
        app.engine
            .execute(qtable_engine::engine::Command::Edit(
                qtable_engine::edit::ProjectEdit::SetOutputStage { output: a, stage },
            ))
            .unwrap();
        app.snapshot = app.engine.snapshot();
        assert_eq!(fraction(&app, a, 200), 0.2);
        assert_eq!(fraction(&app, a, 2000), 1.0);
        assert_eq!(value(&app, TrackId(u64::MAX)), Throughput::default());
        let mut project = Project::default();
        project.main.output = None;
        let mut unset = App::new(Engine::new(project).unwrap(), String::new(), None);
        let id = unset.project().main.id;
        unset.snapshot.output_throughput.push((
            id,
            Throughput {
                messages: 99,
                deferring: true,
                ..Throughput::default()
            },
        ));
        assert_eq!(value(&unset, id), Throughput::default());
    }

    #[test]
    fn throughput_sections_collapse_independently_and_readouts_take_no_focus() {
        let mut app = app();
        let output = app.project().outputs[0].id;
        app.selection.anchor = Some(output);
        let layout = crate::output_detail::view(&app);
        assert_eq!(
            layout.sections.last().unwrap().id,
            crate::detail_layout::THROUGHPUT
        );
        drop(layout);
        app.detail_layout.toggle(
            crate::inspector_panel::Tab::Output,
            crate::detail_layout::THROUGHPUT,
        );
        assert!(
            !crate::output_detail::view(&app)
                .sections
                .last()
                .unwrap()
                .expanded
        );
        assert!(
            crate::main_detail::view(&app)
                .sections
                .iter()
                .find(|s| s.id == crate::detail_layout::THROUGHPUT)
                .unwrap()
                .expanded
        );
        let mut panel = view(&app, output);
        assert!(
            crate::control_focus::test_registry::controls::<_, crate::drafts::DraftKey>(&mut panel)
                .is_empty()
        );
        let mut meter = load(&app, output);
        assert!(
            crate::control_focus::test_registry::controls::<_, crate::drafts::DraftKey>(&mut meter)
                .is_empty()
        );
    }
}
