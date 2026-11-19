//! Shared device geometry; faces supply only their body.
use super::Action;
use crate::control::{Activator, Control, ControlKind, Launch, Look};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::ControlId;
use crate::info::{Help, Id};
use crate::{app::App, messages::Message, style::detail};
use iced::widget::canvas::{Frame, Geometry, Program};
use iced::widget::{Canvas, button, column, container, row, text};
use iced::{Border, Color, Element, Length, Rectangle, Renderer, mouse};
use qtable_engine::model::Device;

fn framed<'a>(
    app: &App,
    width: f32,
    title: Element<'a, Message>,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let theme = app.theme;
    container(column![title, body])
        .width(width)
        .height(Length::Fill)
        .max_height(detail::DEVICE_TITLE_HEIGHT + detail::DEVICE_BODY_HEIGHT)
        .style(move |_| container::Style {
            background: Some(theme.detail_view_background.into()),
            border: Border {
                color: theme.control_contrast_frame,
                width: 1.0,
                radius: theme.corner_device.into(),
            },
            ..Default::default()
        })
        .into()
}

pub(super) fn panel<'a>(
    app: &App,
    name: &'a str,
    width: f32,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let theme = app.theme;
    let title = container(
        text(name)
            .size(detail::TEXT_SIZE)
            .color(theme.control_foreground),
    )
    .padding([2.0, detail::DEVICE_TITLE_INSET])
    .height(detail::DEVICE_TITLE_HEIGHT)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(theme.device_title_background.into()),
        border: crate::control::device_layout::strip_border(theme.corner_device),
        ..Default::default()
    });
    framed(app, width, title.into(), body)
}

pub(super) fn device<'a>(app: &'a App, device: &'a Device, location: View) -> Element<'a, Message> {
    let id = device.id;
    let name: Element<'a, Message> = if app.device_chain.renaming == Some(id) {
        crate::style::input("Device name", &app.device_chain.draft)
            .id(super::rename_id(id))
            .size(detail::TEXT_SIZE)
            .padding([0, 4])
            .on_input(|s| Message::Device(Action::Draft(s)))
            .on_submit(Message::Device(Action::CommitRename))
            .into()
    } else {
        text(&device.name).size(detail::TEXT_SIZE).into()
    };
    let theme = app.theme;
    let activator = app.controls().activator(
        Control::new(
            ControlId::action(View::Track, "device-on", ControlOwner::Device(id)),
            Id::DeviceActivator,
            Activator,
        )
        .lit(device.active),
        Message::Device(Action::Toggle(id)),
    );
    let selected = app.selection.devices.contains(&id);
    let ink = if selected {
        crate::control::theme::ink(app.theme.selection_background)
    } else {
        theme.control_foreground
    };
    let title = container(
        row![
            activator,
            container(name).width(Length::Fill),
            app.controls().wrap(
                ControlId::action(location, "device-fold", ControlOwner::Device(id)),
                false,
                button(
                    Canvas::new(Disclosure {
                        expanded: device.expanded,
                        ink,
                    })
                    .width(detail::DEVICE_DISCLOSURE_SIZE)
                    .height(detail::DEVICE_DISCLOSURE_SIZE),
                )
                .width(detail::DEVICE_DISCLOSURE_SIZE)
                .height(detail::DEVICE_DISCLOSURE_SIZE)
                .padding(0)
                .style(move |_, _| button::Style {
                    text_color: ink,
                    ..Default::default()
                })
                .on_press(Message::Device(Action::Target(
                    super::target_face::Action::Fold(id)
                )))
                .info(Id::TargetFold, Message::Info),
            )
        ]
        .spacing(detail::DEVICE_TITLE_SPACING)
        .align_y(iced::Alignment::Center),
    )
    .padding([0.0, detail::DEVICE_TITLE_INSET])
    .height(detail::DEVICE_TITLE_HEIGHT)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(
            if selected {
                app.theme.selection_background
            } else {
                theme.device_title_background
            }
            .into(),
        ),
        text_color: Some(ink),
        border: crate::control::device_layout::strip_border(theme.corner_device),
        ..Default::default()
    });
    framed(
        app,
        super::face::width(device),
        title.into(),
        super::face::view(app, device, location),
    )
}

struct Disclosure {
    expanded: bool,
    ink: Color,
}

impl<M> Program<M> for Disclosure {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if self.expanded {
            frame.translate(iced::Vector::new(bounds.width, bounds.height));
            frame.rotate(std::f32::consts::PI);
        }
        Launch { follow: false }.paint(
            &mut frame,
            Rectangle::with_size(bounds.size()),
            &Look::glyph(Color::TRANSPARENT, self.ink),
        );
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced::advanced::{Layout, layout, renderer::Headless, widget::Tree};
    use qtable_engine::edit::{ProjectEdit, TargetEdit};
    use qtable_engine::engine::{Command, Engine};
    use qtable_engine::model::{FieldType, NewDevice};

    fn pixels(mut widget: Element<'_, Message>) -> Vec<u8> {
        let mut renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&widget);
        let size = Size::new(1000.0, 189.0);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        widget.as_widget().draw(
            &tree,
            &mut renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(size),
        );
        renderer.screenshot(Size::new(1000, 189), 1.0, Color::BLACK)
    }

    #[test]
    fn device_and_cell_strips_share_a_clear_step_above_the_body() {
        let mut app = App::new(
            Engine::new(crate::files::new_project()).unwrap(),
            "120".into(),
            None,
        );
        let track = app.project().tracks[0].id;
        crate::edit_actions::run_edit(
            &mut app,
            Command::Edit(ProjectEdit::AddDevice {
                track,
                kind: NewDevice::Target,
                before: None,
            }),
        );
        let id = app.project().track(track).unwrap().devices[0].id;
        for parameter in [false, true] {
            let _ = crate::update::update(
                &mut app,
                Message::Device(Action::Target(super::super::target_face::Action::Add(
                    id, parameter,
                ))),
            );
            if !parameter {
                let entry = super::super::target_face::target(&app, id)
                    .unwrap()
                    .1
                    .events[0]
                    .key
                    .clone();
                crate::edit_actions::run_edit(
                    &mut app,
                    Command::Edit(ProjectEdit::EditTarget {
                        device: id,
                        edit: TargetEdit::AddField {
                            entry,
                            value_type: FieldType::Float,
                        },
                    }),
                );
            }
        }
        let _ = crate::update::update(
            &mut app,
            Message::Device(Action::Target(super::super::target_face::Action::Fold(id))),
        );
        app.selection.devices.clear();
        let theme = app.theme;
        assert_eq!(
            theme.device_title_background.into_rgba8(),
            [86, 86, 86, 255]
        );
        assert!(theme.device_title_background.r / theme.detail_view_background.r > 1.38);
        let check = |image: &[u8], x: usize, y: usize, colour: Color| {
            let at = (y * 1000 + x) * 4;
            assert_eq!(&image[at..at + 4], colour.into_rgba8(), "at {x},{y}");
        };
        let image = pixels(device(
            &app,
            app.project().device(id).unwrap().1,
            View::Track,
        ));
        check(&image, 80, 2, theme.device_title_background);
        check(&image, 2, 100, theme.detail_view_background);
        for x in [6, 128] {
            check(&image, x, 42, theme.device_title_background);
            check(&image, x, 59, theme.detail_view_background);
        }
        let track = app.project().track(track).unwrap();
        let image = pixels(panel(
            &app,
            "Mixer",
            crate::mixer_device::width(track),
            crate::mixer_device::view(&app, track, View::Track),
        ));
        check(&image, 80, 2, theme.device_title_background);
        check(&image, 2, 100, theme.detail_view_background);
        for x in [6, 356] {
            check(&image, x, 26, theme.device_title_background);
            check(&image, x, 44, theme.detail_view_background);
        }
    }
}
