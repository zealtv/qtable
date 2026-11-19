//! Bottom status strip and the lifetime of command/scheduler messages.
use std::time::{Duration, Instant};

use iced::widget::{Space, container, row, text, tooltip};
use iced::{Alignment, Element, Length};
use qtable_engine::status::EngineStatus;

use crate::Message;

const MESSAGE_LIFETIME: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Notice,
    Plain,
}

struct StatusMessage {
    text: String,
    severity: Severity,
    shown_at: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct StatusLine {
    message: Option<StatusMessage>,
    seen_error_count: u64,
    seen_warning_count: u64,
    seen_failure: Option<String>,
    /// Every error `show_error` was asked to show, including those the persistent scheduler
    /// failure kept off the bar. A caller that needs to know whether an action reported an error
    /// compares this before and after, rather than reading what the bar shows.
    errors_reported: u64,
}

impl StatusLine {
    /// A project just opened: its first validation error, else its load notice.
    pub(crate) fn for_project(project: &qtable_engine::model::Project) -> Self {
        let mut status = Self::default();
        if let Err(error) = project.validate() {
            status.show_error(error.to_string());
        } else if let Some(notice) = project.every_frame_rate_notice() {
            status.show(notice, Severity::Notice);
        }
        status
    }

    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_ref().map(|message| message.text.as_str())
    }

    pub(crate) fn severity(&self) -> Severity {
        self.message
            .as_ref()
            .map_or(Severity::Plain, |m| m.severity)
    }

    /// How many errors have been reported so far, shown or not.
    pub(crate) fn errors_reported(&self) -> u64 {
        self.errors_reported
    }

    pub(crate) fn show_error(&mut self, text: impl Into<String>) {
        self.show(text, Severity::Error);
    }

    pub(crate) fn show(&mut self, text: impl Into<String>, severity: Severity) {
        self.show_at(text.into(), severity, Instant::now());
    }

    fn show_at(&mut self, text: String, severity: Severity, now: Instant) {
        if severity == Severity::Error {
            self.errors_reported += 1;
        }
        // A stopped scheduler remains the actionable problem until explicitly dismissed.
        if !self.is_persistent() {
            self.message = Some(StatusMessage {
                text,
                severity,
                shown_at: Some(now),
            });
        }
    }

    fn is_persistent(&self) -> bool {
        self.message
            .as_ref()
            .is_some_and(|message| message.shown_at.is_none())
    }

    /// Command/scheduler messages take priority over the current routing reminder.
    fn display(&self, project: &qtable_engine::model::Project) -> (String, Severity) {
        if let Some(message) = self.message() {
            return (
                message
                    .strip_prefix("invalid project: ")
                    .unwrap_or(message)
                    .to_owned(),
                self.severity(),
            );
        }
        let disabled: Vec<_> = project
            .outputs
            .iter()
            .filter(|o| !o.mixer.active)
            .map(|o| o.name.as_str())
            .collect();
        if disabled.is_empty() {
            (String::new(), Severity::Plain)
        } else {
            (
                format!("Outputs off: {}", disabled.join(", ")),
                Severity::Notice,
            )
        }
    }

    /// Successful commands cannot recover a failed scheduler.
    pub(crate) fn clear_transient(&mut self) {
        if !self.is_persistent() {
            self.clear();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.message = None;
    }

    /// Snapshot errors are retained by the engine. Display each new occurrence once,
    /// so dismissal, command success and expiry are not undone by the next poll.
    pub(crate) fn poll(&mut self, status: &EngineStatus, now: Instant) {
        if self.message.as_ref().is_some_and(|message| {
            message
                .shown_at
                .is_some_and(|at| now.saturating_duration_since(at) >= MESSAGE_LIFETIME)
        }) {
            self.clear();
        }
        if status.scheduler_failure != self.seen_failure {
            self.seen_failure.clone_from(&status.scheduler_failure);
            if let Some(failure) = &status.scheduler_failure {
                self.message = Some(StatusMessage {
                    text: format!("Scheduler stopped: {failure}"),
                    severity: Severity::Error,
                    shown_at: None,
                });
            }
        } else if status.error_count != self.seen_error_count
            && status.scheduler_failure.is_none()
            && let Some(error) = &status.last_error
        {
            self.show_at(
                format!("{error} ({} errors)", status.error_count),
                Severity::Error,
                now,
            );
        }
        if status.warning_count != self.seen_warning_count
            && status.scheduler_failure.is_none()
            && status.error_count == self.seen_error_count
            && let Some(warning) = &status.last_warning
        {
            let new = status.warning_count.saturating_sub(self.seen_warning_count) as usize;
            let from = status.recent_warnings.len().saturating_sub(new);
            let message = if status.recent_warnings.is_empty() {
                warning.clone()
            } else {
                status.recent_warnings[from..].join("; ")
            };
            self.show_at(message, Severity::Notice, now);
        }
        self.seen_warning_count = status.warning_count;
        self.seen_error_count = status.error_count;
    }
}

/// Live's mixer icon and its menu arrow: the icon shows or hides every checked strip row, the
/// arrow opens the sections menu.
fn mixer_icon<'a>(
    controls: crate::drafts::Controls<'a>,
    chrome: crate::chrome::State,
) -> Element<'a, Message> {
    use crate::control::{Button, Face, StripIcon};
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::Control;
    use crate::drafts::ControlId;
    use crate::info::Id;
    let icon = controls.canvas(
        Control::new(
            ControlId::action(View::StatusBar, "strip", ControlOwner::Song),
            Id::StripSwitch,
            StripIcon,
        )
        .lit(chrome.strip_open),
        Some(Message::ToggleRegion(crate::chrome::Region::Strip)),
    );
    let arrow = controls.button(
        Control::new(
            ControlId::action(View::StatusBar, "sections-menu", ControlOwner::Song),
            Id::SectionsMenuArrow,
            Button {
                face: Face::Text("▼".to_owned()),
                latched: false,
            },
        ),
        Message::ToggleSectionsMenu,
    );
    row![icon, arrow]
        .spacing(2)
        .align_y(Alignment::Center)
        .into()
}

pub(crate) fn view<'a>(
    status: &'a StatusLine,
    chrome: crate::chrome::State,
    project: &'a qtable_engine::model::Project,
    controls: crate::drafts::Controls<'a>,
    theme: crate::control::Theme,
) -> Element<'a, Message> {
    let (message, severity) = status.display(project);
    // Keep the strip one line high. Long paths/errors are readable in the tooltip.
    let label = container(
        text(message.replace(['\n', '\r'], " "))
            .size(crate::style::detail::TEXT_SIZE)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::FillPortion(1))
    .padding([0, 6])
    .style(move |_| message_style(theme, severity))
    .height(Length::Fixed(20.0))
    .align_y(Alignment::Center)
    .clip(true);
    let label = crate::control::tooltip(label, message.to_owned(), tooltip::Position::Top);
    let mut strip = row![
        crate::chrome::toggle(
            "ⓘ",
            "Info View (?)",
            crate::chrome::Region::Info,
            chrome.info_open
        ),
        label,
        Space::new().width(Length::FillPortion(1))
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if status.message().is_some() {
        strip = strip.push(crate::info::help(
            crate::style::flat("×").on_press(Message::DismissStatus),
            crate::info::Id::DismissStatus,
        ));
    }
    strip = strip.push(mixer_icon(controls, chrome));
    strip = strip.push(crate::chrome::toggle(
        if chrome.detail_open { "▼" } else { "▲" },
        "Detail View (⌘⌥L)",
        crate::chrome::Region::Detail,
        chrome.detail_open,
    ));
    container(strip)
        .width(Length::Fill)
        .height(Length::Fixed(24.0))
        .padding([2, 6])
        .align_y(Alignment::Center)
        .style(move |_| crate::style::status_strip(theme))
        .into()
}

/// Live's severity backgrounds with dark text on the alert/notice fill.
fn message_style(
    theme: crate::control::Theme,
    severity: Severity,
) -> iced::widget::container::Style {
    let (background, ink) = match severity {
        Severity::Error => (theme.alert, theme.control_on_foreground),
        Severity::Notice => (theme.search_indication, theme.control_on_foreground),
        Severity::Plain => (theme.control_background, theme.control_foreground),
    };
    iced::widget::container::Style {
        text_color: Some(ink),
        background: Some(background.into()),
        border: iced::Border::default().rounded(crate::control::LIVE_DARK.corner_cell),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_messages_omit_project_error_prefix_and_preserve_the_cause() {
        let project = qtable_engine::model::Project::default();
        let mut line = StatusLine::default();
        for (raw, visible) in [
            (
                "invalid project: OSC path must not end with slash",
                "OSC path must not end with slash",
            ),
            (
                "invalid project: path must start with a slash and contain no NUL",
                "path must start with a slash and contain no NUL",
            ),
            (
                "could not open project: permission denied",
                "could not open project: permission denied",
            ),
            ("send failed", "send failed"),
        ] {
            line.show_error(raw);
            assert_eq!(
                line.display(&project),
                (visible.to_owned(), Severity::Error)
            );
            // Keep the engine's original diagnostic available internally.
            assert_eq!(line.message(), Some(raw));
        }
    }

    #[test]
    fn severity_follows_replacement_expiry_and_dismissal() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        for severity in [Severity::Error, Severity::Notice, Severity::Plain] {
            line.show_at("message".into(), severity, now);
            assert_eq!(line.severity(), severity);
            line.poll(&EngineStatus::default(), now + MESSAGE_LIFETIME);
            assert_eq!(line.message(), None);
            assert_eq!(line.severity(), Severity::Plain);
            line.show("message", severity);
            line.clear();
            assert_eq!(line.severity(), Severity::Plain);
        }
    }

    #[test]
    fn routing_notice_returns_after_errors_expire_or_are_dismissed() {
        use qtable_engine::model::{Project, Track, TrackId};
        let mut project = Project::default();
        let mut output = Track::new_output(TrackId(1), "Lights", "127.0.0.1", 9000);
        output.mixer.active = false;
        project.outputs.push(output);
        let now = Instant::now();
        let mut line = StatusLine::default();
        let notice = ("Outputs off: Lights".to_owned(), Severity::Notice);
        assert_eq!(line.display(&project), notice);
        line.show_at("bad path".into(), Severity::Error, now);
        assert_eq!(line.display(&project), ("bad path".into(), Severity::Error));
        line.poll(&EngineStatus::default(), now + MESSAGE_LIFETIME);
        assert_eq!(line.display(&project), notice);
        line.show_error("send failed");
        line.clear();
        assert_eq!(line.display(&project), notice);
        project.outputs[0].mixer.active = true;
        assert_eq!(line.display(&project), (String::new(), Severity::Plain));
    }

    #[test]
    fn an_opened_set_with_other_stream_rates_shows_the_every_frame_notice() {
        use qtable_engine::model::{EVERY_FRAME_RATE_NOTICE, Project, SendMode};
        assert_eq!(StatusLine::for_project(&Project::default()).message(), None);
        let mut project = Project::default();
        let output = &mut project.main.mixer_device.volume.output;
        output.enabled = true;
        output.send_mode = SendMode::EveryFrame;
        output.stream_rate = 50.0;
        let line = StatusLine::for_project(&project);
        assert_eq!(line.message(), Some(EVERY_FRAME_RATE_NOTICE));
        assert_eq!(line.severity(), Severity::Notice);
        // A validation error is the more urgent message.
        project.main.mixer_device.volume.output.stream_rate = 100.0;
        assert_eq!(
            StatusLine::for_project(&project).severity(),
            Severity::Error
        );
    }

    #[test]
    fn status_colours_use_the_named_theme_tokens_and_readable_ink() {
        use iced::{Background, Color};
        let theme = crate::control::LIVE_DARK;
        assert_eq!(theme.alert, Color::from_rgb8(231, 105, 66));
        assert_eq!(theme.search_indication, Color::from_rgb8(236, 202, 109));
        // Overriding the theme must reskin the bar too.
        let custom = crate::control::Theme {
            alert: Color::from_rgb8(200, 50, 30),
            search_indication: Color::from_rgb8(240, 210, 80),
            ..theme
        };
        for t in [theme, custom] {
            for (severity, background, ink) in [
                (Severity::Error, t.alert, t.control_on_foreground),
                (
                    Severity::Notice,
                    t.search_indication,
                    t.control_on_foreground,
                ),
                (Severity::Plain, t.control_background, t.control_foreground),
            ] {
                let style = message_style(t, severity);
                assert_eq!(style.background, Some(Background::Color(background)));
                assert_eq!(style.text_color, Some(ink));
                assert_eq!(
                    style.border.radius,
                    iced::border::Radius::from(crate::control::LIVE_DARK.corner_cell)
                );
                let strip = crate::style::status_strip(t);
                assert_eq!(
                    strip.background,
                    Some(Background::Color(t.control_background))
                );
                assert_eq!(strip.text_color, Some(t.control_foreground));
            }
        }
    }

    #[test]
    fn output_warnings_use_notice_colour_and_stay_dismissed() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        let mut status = EngineStatus {
            last_warning: Some("Lights: OSC message too large for a bundle".into()),
            warning_count: 1,
            ..Default::default()
        };
        line.poll(&status, now);
        assert_eq!(line.message(), status.last_warning.as_deref());
        assert_eq!(line.severity(), Severity::Notice);
        line.clear();
        line.poll(&status, now);
        assert!(line.message().is_none());
        status.warning_count += 2;
        status.recent_warnings = vec!["Main: oversize".into(), "Out: oversize".into()];
        line.poll(&status, now);
        assert_eq!(line.message(), Some("Main: oversize; Out: oversize"));
        line.poll(&status, now + MESSAGE_LIFETIME);
        assert!(line.message().is_none());
    }

    #[test]
    fn messages_expire_without_reappearing_from_retained_snapshot() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        let status = EngineStatus {
            last_error: Some("send failed".into()),
            error_count: 1,
            ..Default::default()
        };
        line.poll(&status, now);
        assert_eq!(line.message(), Some("send failed (1 errors)"));
        assert_eq!(line.severity(), Severity::Error);
        line.poll(&status, now + MESSAGE_LIFETIME - Duration::from_millis(1));
        assert!(line.message().is_some());
        line.poll(&status, now + MESSAGE_LIFETIME);
        assert!(line.message().is_none());
        line.poll(&status, now + MESSAGE_LIFETIME * 2);
        assert!(line.message().is_none());
    }

    #[test]
    fn clearing_preserves_cursor_but_repeated_error_is_new() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        let mut status = EngineStatus {
            last_error: Some("send failed".into()),
            error_count: 1,
            ..Default::default()
        };
        line.poll(&status, now);
        line.clear();
        line.poll(&status, now);
        assert!(line.message().is_none());
        status.error_count += 1;
        line.poll(&status, now);
        assert_eq!(line.message(), Some("send failed (2 errors)"));
    }

    #[test]
    fn errors_are_counted_even_while_the_scheduler_failure_keeps_them_off_the_bar() {
        let mut line = StatusLine::default();
        line.show_error("first");
        line.show("a notice", Severity::Notice);
        assert_eq!(line.errors_reported(), 1);
        let status = EngineStatus {
            scheduler_failure: Some("thread exited".into()),
            ..EngineStatus::default()
        };
        line.poll(&status, Instant::now());
        line.show_error("dropped by the bar");
        assert_eq!(line.message(), Some("Scheduler stopped: thread exited"));
        assert_eq!(line.errors_reported(), 2);
    }

    #[test]
    fn scheduler_failure_takes_priority_and_can_be_dismissed() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        let mut status = EngineStatus {
            last_error: Some("send failed".into()),
            error_count: 1,
            scheduler_failure: Some("thread exited".into()),
            ..Default::default()
        };
        line.poll(&status, now);
        assert_eq!(line.message(), Some("Scheduler stopped: thread exited"));
        status.error_count += 1;
        line.poll(&status, now);
        assert_eq!(line.message(), Some("Scheduler stopped: thread exited"));
        line.clear();
        line.poll(&status, now);
        assert!(line.message().is_none());
    }

    #[test]
    fn scheduler_failure_survives_expiry_success_and_other_messages() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        let status = EngineStatus {
            scheduler_failure: Some("thread exited".into()),
            ..Default::default()
        };
        line.poll(&status, now);
        line.poll(&status, now + MESSAGE_LIFETIME * 100);
        line.clear_transient();
        line.show("notice", Severity::Notice);
        line.show("plain", Severity::Plain);
        assert_eq!(line.severity(), Severity::Error);
        line.show_at(
            "another error".into(),
            Severity::Error,
            now + MESSAGE_LIFETIME * 100,
        );
        assert_eq!(line.message(), Some("Scheduler stopped: thread exited"));
        line.clear();
        line.poll(&status, now + MESSAGE_LIFETIME * 101);
        assert!(line.message().is_none());
        line.show_at(
            "another error".into(),
            Severity::Error,
            now + MESSAGE_LIFETIME * 101,
        );
        assert_eq!(line.message(), Some("another error"));
        line.poll(&status, now + MESSAGE_LIFETIME * 102);
        assert!(line.message().is_none());
    }

    #[test]
    fn replacement_message_gets_its_own_lifetime() {
        let now = Instant::now();
        let mut line = StatusLine::default();
        line.show_at("old".into(), Severity::Error, now);
        line.show_at("new".into(), Severity::Error, now + MESSAGE_LIFETIME);
        line.poll(&EngineStatus::default(), now + MESSAGE_LIFETIME);
        assert_eq!(line.message(), Some("new"));
        line.poll(&EngineStatus::default(), now + MESSAGE_LIFETIME * 2);
        assert!(line.message().is_none());
    }
}
