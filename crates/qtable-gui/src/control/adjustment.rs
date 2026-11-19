//! Shared numeric steps and held-arrow gestures. Repetition follows native key events.
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::canvas;
use qtable_engine::model::{ParameterKind, ParameterSpec};

pub(super) type TextAdjustment = std::rc::Rc<dyn Fn(&str, bool, bool) -> Option<String>>;

pub(crate) fn percent(text: &str, up: bool, fine: bool) -> Option<String> {
    percent_in(0.0, 100.0)(text, up, fine)
}

/// `percent`'s arrow-key step for a percentage field whose range is `low` to `high`, in whole
/// percent: the mapping inspector's 1–1000 % sensitivity.
pub(crate) fn percent_in(low: f64, high: f64) -> impl Fn(&str, bool, bool) -> Option<String> {
    move |text, up, fine| {
        let value = text
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .ok()?;
        value
            .is_finite()
            .then(|| format!("{} %", Nudge::integer(low, high).next(value, up, fine)))
    }
}

pub(super) const FINE_FACTOR: f64 = 0.25;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Nudge {
    min: f64,
    max: f64,
    step: f64,
    quantum: f64,
}
impl Nudge {
    pub(crate) const FLOAT: Self = Self::continuous(-f64::MAX, f64::MAX, 0.01);
    pub(crate) const NORMALIZED: Self = Self::continuous(0.0, 1.0, 0.01);
    pub(crate) const fn continuous(min: f64, max: f64, step: f64) -> Self {
        Self {
            min,
            max,
            step,
            quantum: 0.0,
        }
    }
    pub(crate) const fn integer(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            step: 1.0,
            quantum: 1.0,
        }
    }
    pub(crate) fn from_spec(spec: &ParameterSpec) -> Self {
        let step = match spec.kind {
            ParameterKind::Continuous => (spec.max - spec.min) / 100.0,
            ParameterKind::Quantised { step } => step,
            ParameterKind::Enum(_) => 1.0,
        };
        Self {
            min: spec.min,
            max: spec.max,
            step,
            quantum: if matches!(spec.kind, ParameterKind::Continuous) {
                0.0
            } else {
                step
            },
        }
    }
    /// A Slider/Dial publishes a fraction while its argument has its own value range.
    pub(crate) fn fraction(self) -> Self {
        let span = self.max - self.min;
        Self {
            min: 0.0,
            max: 1.0,
            step: self.step / span,
            quantum: self.quantum / span,
        }
    }
    pub(super) fn next(self, value: f64, up: bool, fine: bool) -> f64 {
        if !value.is_finite() {
            return value;
        }
        let step = if fine {
            (self.step * FINE_FACTOR).max(self.quantum)
        } else {
            self.step
        };
        let next = value + if up { step } else { -step };
        let next = if self.quantum > 0.0 {
            self.min + ((next - self.min) / self.quantum).round() * self.quantum
        } else {
            next
        };
        // Remove arithmetic noise without rounding away finer steps or precise origins.
        let rounded = (next * 1e12).round() / 1e12;
        let next = if rounded.is_finite() && (rounded - next).abs() < step * 1e-6 {
            rounded
        } else {
            next
        };
        next.clamp(self.min, self.max)
    }
}

pub(super) fn arrow(key: &Key, modifiers: Modifiers) -> Option<bool> {
    if modifiers.intersects(Modifiers::ALT | Modifiers::CTRL | Modifiers::LOGO) {
        return None;
    }
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => Some(true),
        Key::Named(Named::ArrowDown) => Some(false),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Keyboard {
    held: Option<(Named, f64)>,
}
pub(super) enum Adjustment {
    Change(f64),
    Release(f64),
    Capture,
}
impl Adjustment {
    pub(super) fn action<M>(
        self,
        change: impl Fn(f64) -> M,
        release: impl Fn(f64) -> M,
        capture: bool,
    ) -> canvas::Action<M> {
        let action = match self {
            Self::Change(v) => canvas::Action::publish(change(v)),
            Self::Release(v) => canvas::Action::publish(release(v)),
            Self::Capture => canvas::Action::capture(),
        };
        if capture {
            action.and_capture()
        } else {
            action
        }
    }
}
impl Keyboard {
    pub(super) fn update(
        &mut self,
        event: &canvas::Event,
        selected: bool,
        dragging: bool,
        value: f64,
        nudge: Nudge,
    ) -> Option<(Adjustment, bool)> {
        if !selected || matches!(event, canvas::Event::Window(iced::window::Event::Unfocused)) {
            return self
                .held
                .take()
                .map(|(_, v)| (Adjustment::Release(v), false));
        }
        match event {
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) => {
                let up = arrow(key, *modifiers)?;
                if dragging {
                    return Some((Adjustment::Capture, true));
                }
                if *repeat && self.held.is_none() {
                    return Some((Adjustment::Capture, true));
                }
                let current = self.held.map_or(value, |(_, v)| v);
                let next = nudge.next(current, up, modifiers.shift());
                self.held = Some((if up { Named::ArrowUp } else { Named::ArrowDown }, next));
                Some((
                    if next != current {
                        Adjustment::Change(next)
                    } else {
                        Adjustment::Capture
                    },
                    true,
                ))
            }
            canvas::Event::Keyboard(keyboard::Event::KeyReleased {
                key: Key::Named(key),
                ..
            }) if self.held.is_some_and(|(held, _)| held == *key) => self
                .held
                .take()
                .map(|(_, v)| (Adjustment::Release(v), true)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn press(up: bool, fine: bool, repeat: bool) -> canvas::Event {
        canvas::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(if up { Named::ArrowUp } else { Named::ArrowDown }),
            modified_key: Key::Named(if up { Named::ArrowUp } else { Named::ArrowDown }),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: if fine {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            },
            text: None,
            repeat,
        })
    }
    #[test]
    fn typed_axes_keep_integer_steps_float_range_steps_and_endpoints() {
        assert_eq!(Nudge::integer(-10.0, 10.0).next(3.0, true, true), 4.0);
        assert_eq!(Nudge::NORMALIZED.next(0.5, true, false), 0.51);
        assert_eq!(Nudge::NORMALIZED.next(0.5, false, true), 0.4975);
        assert_eq!(Nudge::NORMALIZED.next(0.999, true, false), 1.0);
        assert_eq!(Nudge::NORMALIZED.next(0.0, false, true), 0.0);
        let spec = ParameterSpec {
            min: -50.0,
            max: 150.0,
            ..qtable_engine::model::VOLUME
        };
        assert_eq!(Nudge::from_spec(&spec).next(0.0, true, false), 2.0);
        assert_eq!(
            Nudge::integer(-10.0, 10.0).fraction().next(0.5, true, true),
            0.55
        );
        assert_eq!(percent("50 %", true, true).as_deref(), Some("51 %"));
        assert!(percent("-", true, false).is_none());
    }
    #[test]
    fn held_keys_capture_repeats_and_finish_once_on_blur_or_selection_loss() {
        for finish in [
            canvas::Event::Window(iced::window::Event::Unfocused),
            canvas::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
        ] {
            let mut keyboard = Keyboard::default();
            let change = press(true, false, false);
            assert!(
                keyboard
                    .update(&change, false, false, 0.5, Nudge::NORMALIZED)
                    .is_none()
            );
            assert!(matches!(
                keyboard.update(&change, true, false, 0.5, Nudge::NORMALIZED),
                Some((Adjustment::Change(0.51), true))
            ));
            assert!(matches!(
                keyboard.update(
                    &press(true, true, true),
                    true,
                    false,
                    0.5,
                    Nudge::NORMALIZED
                ),
                Some((Adjustment::Change(0.5125), true))
            ));
            assert!(matches!(
                keyboard.update(&finish, false, false, 0.5, Nudge::NORMALIZED),
                Some((Adjustment::Release(0.5125), false))
            ));
            assert!(
                keyboard
                    .update(&finish, false, false, 0.5, Nudge::NORMALIZED)
                    .is_none()
            );
        }
    }
    #[test]
    fn pointer_drags_keep_arrows_and_repeats_cannot_begin_on_new_selection() {
        let mut keyboard = Keyboard::default();
        assert!(matches!(
            keyboard.update(
                &press(true, false, false),
                true,
                true,
                0.5,
                Nudge::NORMALIZED
            ),
            Some((Adjustment::Capture, true))
        ));
        assert!(keyboard.held.is_none());
        assert!(matches!(
            keyboard.update(
                &press(true, false, true),
                true,
                false,
                0.5,
                Nudge::NORMALIZED
            ),
            Some((Adjustment::Capture, true))
        ));
        assert_eq!(arrow(&Key::Named(Named::ArrowUp), Modifiers::COMMAND), None);
    }
}
