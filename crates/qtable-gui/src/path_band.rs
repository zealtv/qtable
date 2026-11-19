//! The Path row: every column's Path, edited in place. A path is the address segment a column
//! adds to its events (destination, then groups, then track, then the event's own address), so
//! every kind has one, Main and outputs included. Enter or blur commits, Escape reverts, and a
//! rejected draft stays with the error look.
use crate::app::App;
use crate::control::path_field::Prefix;
use crate::control::{PathField, SizeRole};
use crate::control_focus::View;
use crate::drafts::TrackField;
use crate::drafts::{Control, ControlId, DraftKey};
use crate::info::{Id, Subject};
use crate::messages::{EditMessage, Message};
use crate::style::{self, detail};
use iced::widget::{column, container, text};
use iced::{Element, Length};
use qtable_engine::model::{PathLevel, TrackId};

pub(crate) fn path(app: &App, track: TrackId) -> Element<'_, Message> {
    let Some(model) = app.project().track(track) else {
        return iced::widget::Space::new().into();
    };
    let key = DraftKey::Track(track, TrackField::Path);
    let value = crate::view::name_field_value(&app.drafts.values, &key, model.path.as_str());
    let error = app.drafts.address_error(key, model.path.validate());
    let field = app.controls().path(
        Control::new(
            ControlId::field(View::Mixer, key),
            Id::Path,
            PathField {
                placeholder: "no path",
                prefix: Prefix::of(app.project(), track, PathLevel::Track, false),
                absolute: None,
                subject: Subject::Path(track),
            },
        )
        .role(SizeRole::Strip)
        .invalid(error.is_some()),
        &value,
        move |text| Message::Edit(EditMessage::PathChanged(track, text)),
        Message::Edit(EditMessage::PathSubmitted(track, value.clone())),
        Message::Edit(EditMessage::FocusField(key)),
        None,
    );
    container(
        column![
            text("Path")
                .size(detail::COMPACT_TEXT_SIZE)
                .color(style::DISABLED)
                .wrapping(text::Wrapping::None),
            field
        ]
        .spacing(detail::LABEL_GAP),
    )
    .padding(detail::STRIP_INSET)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use iced::advanced::{layout, renderer::Headless, widget::Tree};
    use iced::{Renderer, Size};
    use qtable_engine::engine::Engine;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    fn app() -> App {
        let project = qtable_engine::file::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/loaded-invalid.ron"
        ))
        .unwrap();
        App::new(Engine::new(project).unwrap(), "120".into(), None)
    }

    #[test]
    fn focus_field_returns_a_task_that_focuses_and_moves_the_cursor() {
        let mut app = app();
        let key = DraftKey::Track(TrackId(1), TrackField::Path);
        let task = crate::update::update(&mut app, Message::Edit(EditMessage::FocusField(key)));
        assert!(task.units() > 0);
    }

    #[test]
    fn the_strip_path_row_builds_for_track_group_output_and_main() {
        let app = app();
        let mut future = std::pin::pin!(Renderer::new(
            iced::Font::default(),
            iced::Pixels(detail::TEXT_SIZE as f32),
            Some("tiny-skia")
        ));
        let Poll::Ready(Some(renderer)) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("software renderer must be available");
        };
        let project = app.project();
        let ids = [
            project.tracks.iter().find(|t| !t.is_group()).unwrap().id,
            project.group_tracks().next().unwrap().id,
            project.outputs[0].id,
            project.main.id,
        ];
        for id in ids {
            let mut row = path(&app, id);
            let mut tree = Tree::new(&row);
            let node = row.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(104.0, 200.0)),
            );
            assert!(
                node.size().width.is_finite() && node.size().height > 0.0,
                "{id:?}"
            );
        }
    }
}
