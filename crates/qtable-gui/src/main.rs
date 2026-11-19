use crate::{
    app::App,
    fields::format_tempo,
    messages::{FileMessage, Message},
    performance::tick_interval,
    update::update,
    view::view,
};
use iced::Subscription;
use qtable_engine::engine::Engine;
use std::sync::{Arc, Mutex};

mod app;
mod browser;
mod capture;
mod chrome;
mod clip_actions;
mod clip_shortcuts;
mod context_menu;
mod control;
mod control_focus;
mod delay_fields;
mod detail_layout;
mod device_chain;
mod dividers;
mod drafts;
mod edit_actions;
mod event_feedback;
mod field_focus;
mod fields;
mod files;
mod focus;
mod gesture_clock;
mod gesture_steps;
mod grid;
mod header_actions;
mod info;
mod inspector;
mod inspector_panel;
mod inspector_update;
mod main_detail;
mod map_mode;
mod menu_keys;
mod messages;
mod mixer_band;
mod mixer_device;
mod output_band;
mod output_detail;
mod outputs;
mod overlay;
mod parameter_gesture;
mod path_band;
mod path_context;
mod performance;
mod remote;
mod scene_stop_feedback;
mod sections_menu;
mod selection;
mod settings_pane;
mod shortcuts;
mod stage_settings;
mod status;
mod strip_rows;
mod style;
mod throughput;
mod track_clipboard;
mod track_detail;
mod transport;
mod typing;
mod undo;
mod update;
mod view;

fn main() -> iced::Result {
    let (initial_path, chrome) =
        chrome::startup(std::env::args_os().skip(1)).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(2);
        });
    let project = match &initial_path {
        Some(path) => qtable_engine::file::load(path).unwrap_or_else(|error| {
            eprintln!("could not open project: {error}");
            std::process::exit(2);
        }),
        None => capture::fixture_project().unwrap_or_else(files::new_project),
    };
    let initial_tempo = format_tempo(project.tempo.bpm());
    let engine = Engine::new(project).unwrap_or_else(|error| {
        eprintln!("could not start qtable engine: {error}");
        std::process::exit(2);
    });
    let engine = Arc::new(Mutex::new(Some(engine)));
    iced::application(
        move || {
            let mut app = App::new(
                // The boot closure runs once; no other thread can poison this mutex.
                engine
                    .lock()
                    .unwrap()
                    .take()
                    .expect("application boots once"),
                initial_tempo.clone(),
                initial_path.clone(),
            );
            app.chrome = chrome;
            if let Err(error) = app.remote.start() {
                app.status.show_error(error);
            }
            capture::apply(app)
        },
        update,
        view,
    )
    .window(iced::window::Settings {
        min_size: Some(iced::Size::new(480.0, dividers::MIN_WINDOW_HEIGHT)),
        icon: window_icon(),
        platform_specific: platform_specific_window(),
        ..iced::window::Settings::default()
    })
    .window_size(capture::window_size().unwrap_or(iced::Size::new(1024.0, 768.0)))
    .title(App::title)
    .exit_on_close_request(false)
    .subscription(subscription)
    .theme(|app: &App| style::live_theme(&app.theme))
    .settings(iced::Settings {
        default_text_size: iced::Pixels(style::detail::TEXT_SIZE as f32),
        ..iced::Settings::default()
    })
    .run()
}

/// The window icon: 128 by 128 straight-alpha RGBA, built from `assets/icon/qtable-icon.svg` by
/// `assets/icon/build.sh`. X11 and Windows show it; macOS takes the Dock icon from the app bundle
/// (`tools/package-macos.sh` builds one), and Wayland takes it from the desktop file.
fn window_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(
        include_bytes!("../assets/window-icon.rgba").to_vec(),
        128,
        128,
    )
    .ok()
}

/// On Linux the application id names the window to the desktop, which matches it to
/// `assets/linux/qtable.desktop` (and so to its icon) under Wayland as well as X11.
#[cfg(target_os = "linux")]
fn platform_specific_window() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        application_id: "qtable".to_string(),
        ..Default::default()
    }
}

#[cfg(not(target_os = "linux"))]
fn platform_specific_window() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific::default()
}

fn subscription(app: &App) -> Subscription<Message> {
    Subscription::batch([
        remote::subscription(&app.remote),
        iced::time::every(tick_interval(app)).map(|_| Message::Tick),
        iced::window::close_requests()
            .map(|value| Message::Files(FileMessage::CloseRequested(value))),
        iced::event::listen_with(|event, status, _| window_event(event, status)),
        iced::keyboard::listen().map(|event| match event {
            iced::keyboard::Event::ModifiersChanged(modifiers)
            | iced::keyboard::Event::KeyPressed { modifiers, .. }
            | iced::keyboard::Event::KeyReleased { modifiers, .. } => {
                Message::ModifiersChanged(modifiers)
            }
        }),
    ])
}

/// Subscription-side shortcuts run only after the window's key gate and field widgets.
fn window_event(event: iced::Event, status: iced::event::Status) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(_))
        | iced::Event::Touch(iced::touch::Event::FingerPressed { .. }) => {
            Some(Message::PointerPressed)
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(Message::Browser(crate::browser::Action::Release))
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(_))
        | iced::Event::Touch(iced::touch::Event::FingerLifted { .. }) => {
            Some(Message::CheckFieldFocus)
        }
        iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::ReleaseKeys),
        iced::Event::Window(iced::window::Event::Resized(size)) => {
            Some(Message::WindowResized(size))
        }
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        iced::Event::Mouse(iced::mouse::Event::CursorLeft) => {
            Some(Message::Browser(browser::Action::Cancel))
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyReleased { key, .. }) => {
            Some(Message::KeyReleased(key))
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            ref key,
            ref modified_key,
            modifiers,
            repeat,
            ..
        }) if status == iced::event::Status::Ignored
            && chrome::event_shortcut(key, modified_key, modifiers, repeat, status).is_some() =>
        {
            chrome::event_shortcut(key, modified_key, modifiers, repeat, status)
                .map(Message::ToggleRegion)
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat,
            ..
        }) => Some(Message::KeyPressed {
            key,
            modifiers,
            repeat,
            ignored: status == iced::event::Status::Ignored,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
