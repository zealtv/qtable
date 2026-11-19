//! Startup-only held gestures. Samples feed production gesture/timer/planner
//! helpers on native redraws; no pointer events or replacement drawings.
//!
//! Without the `dev-hooks` feature nothing constructs a `Capture`, so the gesture code that
//! consults one never runs.
#![cfg_attr(not(any(test, feature = "dev-hooks")), allow(dead_code))]
use iced::{Point, Rectangle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) struct Capture {
    start: Option<Instant>,
    before: bool,
    pub drop: bool,
    pub done: bool,
}

pub(super) struct Sample {
    pub first: bool,
    pub finish: bool,
    pub at: Point,
}

impl Capture {
    pub fn from_scenario(scenario: &str, axis: &str) -> Option<Self> {
        if !cfg!(any(test, feature = "dev-hooks")) {
            return None;
        }
        let mode = scenario.strip_prefix(axis)?;
        matches!(mode, "-before" | "-preview" | "-drop").then_some(Self {
            start: None,
            before: mode == "-before",
            drop: mode == "-drop",
            done: false,
        })
    }
    pub fn sample(
        &mut self,
        now: Instant,
        visible: Rectangle,
        header_h: f32,
        horizontal: bool,
    ) -> Sample {
        let first = self.start.is_none();
        let start = *self.start.get_or_insert(now);
        let finish =
            self.before || now.saturating_duration_since(start) >= Duration::from_millis(1300);
        self.done = finish;
        Sample {
            first,
            finish,
            at: Point::new(
                visible.x + visible.width - 8.,
                if horizontal {
                    visible.y + header_h - 4.
                } else {
                    visible.y + visible.height - 8.
                },
            ),
        }
    }
}

#[cfg(any(test, feature = "dev-hooks"))]
pub(super) fn setup(app: &mut crate::app::App, scenario: &str) -> bool {
    use super::Action;
    use crate::messages::Message;
    if let Some(capture) = Capture::from_scenario(scenario, "autoscroll-track") {
        if let Some(track) = app.project().tracks.first().map(|t| t.id) {
            let _ = crate::update::update(app, Message::Grid(Action::SelectTrack(track, false)));
            app.header_gestures.tracks.capture.set(Some(capture));
        }
        true
    } else if Capture::from_scenario(scenario, "autoscroll-scene").is_some() {
        if let Some(scene) = app.project().scenes.get(1).map(|s| s.id) {
            let _ = crate::update::update(app, Message::Grid(Action::SelectScene(scene)));
        }
        true
    } else {
        false
    }
}
