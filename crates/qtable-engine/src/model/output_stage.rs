//! How an output paces what it sends (`output-stage-design`, 2026-10-15): the frame clock,
//! the Budget, Bundle and Refresh. The settings live on the destination component, so output
//! tracks and Main share them; the scheduler's output stage reads them.
use super::*;

pub const FRAME_RATE: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Frame Rate"),
    min: 1.0,
    max: 60.0,
    default: 30.0,
    kind: ParameterKind::Continuous,
    unit: Unit::Hertz,
};

/// Messages per second the output may send, clip events included. `default` is the value held
/// until one is typed.
pub const BUDGET: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Budget"),
    min: 10.0,
    max: 10_000.0,
    default: 1000.0,
    kind: ParameterKind::Quantised { step: 1.0 },
    unit: Unit::MessagesPerSecond,
};

/// Keep-alive interval. `default` is the value held until one is typed: the lighting protocols'
/// one-second figure (references §1).
pub const REFRESH: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Refresh"),
    min: 0.1,
    max: 10.0,
    default: 1.0,
    kind: ParameterKind::Continuous,
    unit: Unit::Seconds,
};

/// Every field is omitted at its default; format 7 saves require v3.2 even without stage settings.
/// Budget and Refresh keep their last typed value while Off, as `stream_rate` does across
/// send modes, so switching one back on restores it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputStage {
    #[serde(skip_serializing_if = "is_default_frame_rate")]
    pub frame_rate: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub budget_on: bool,
    /// Last typed Budget, retained while Off.
    #[serde(skip_serializing_if = "is_default_budget")]
    pub budget: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bundle: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub refresh_on: bool,
    /// Last typed Refresh interval in seconds, retained while Off.
    #[serde(skip_serializing_if = "is_default_refresh")]
    pub refresh: f64,
}
impl Default for OutputStage {
    fn default() -> Self {
        Self {
            frame_rate: FRAME_RATE.default,
            budget_on: false,
            budget: BUDGET.default as u32,
            bundle: false,
            refresh_on: false,
            refresh: REFRESH.default,
        }
    }
}
impl OutputStage {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Active Budget; None while Off, whatever value it holds.
    pub fn budget(&self) -> Option<u32> {
        self.budget_on.then_some(self.budget)
    }

    /// Active Refresh interval; None while Off, whatever value it holds.
    pub fn refresh(&self) -> Option<f64> {
        self.refresh_on.then_some(self.refresh)
    }

    pub fn frame_rate_parameter(&self) -> Parameter<'static> {
        Parameter::new(&FRAME_RATE, self.frame_rate)
    }

    /// The Budget field: its held value, disabled while Off.
    pub fn budget_parameter(&self) -> Parameter<'static> {
        Parameter {
            is_enabled: self.budget_on,
            ..Parameter::new(&BUDGET, f64::from(self.budget))
        }
    }

    /// The Refresh field: its held value, disabled while Off.
    pub fn refresh_parameter(&self) -> Parameter<'static> {
        Parameter {
            is_enabled: self.refresh_on,
            ..Parameter::new(&REFRESH, self.refresh)
        }
    }

    /// Held values are checked whether on or off, as `stream_rate` is in every send mode.
    pub fn validate(&self) -> ModelResult {
        FRAME_RATE.check(self.frame_rate).map_err(ModelError)?;
        BUDGET.check(f64::from(self.budget)).map_err(ModelError)?;
        REFRESH.check(self.refresh).map_err(ModelError)?;
        Ok(())
    }
}

fn is_default_budget(budget: &u32) -> bool {
    f64::from(*budget) == BUDGET.default
}

fn is_default_refresh(refresh: &f64) -> bool {
    *refresh == REFRESH.default
}

fn is_default_frame_rate(rate: &f64) -> bool {
    *rate == FRAME_RATE.default
}

impl Track {
    /// The output stage of a track with a destination; None for any other kind, and for Main
    /// while it has no destination.
    pub fn output_stage(&self) -> Option<&OutputStage> {
        self.output.as_ref().map(|o| &o.stage)
    }
}

/// Posted once when a set loads whose Every Frame rates no longer apply (open question 9).
pub const EVERY_FRAME_RATE_NOTICE: &str = "Every Frame now follows each output's Frame Rate";

impl Project {
    /// The load notice, when an enabled Every Frame parameter's stored v3.0 rate differs from
    /// the Frame Rate of an output it sends to. At the defaults (30 Hz and 30 Hz) there is none.
    pub fn every_frame_rate_notice(&self) -> Option<&'static str> {
        let differs = self
            .tracks
            .iter()
            .chain(&self.outputs)
            .chain([&self.main])
            .flat_map(Track::parameters)
            .any(|target| {
                let Some(rate) = self.parameter(&target).and_then(|p| p.output.stream_rate())
                else {
                    return false;
                };
                self.parameter_recipients(&target)
                    .into_iter()
                    .filter_map(Track::output_stage)
                    .any(|stage| stage.frame_rate != rate)
            });
        differs.then_some(EVERY_FRAME_RATE_NOTICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streaming(hz: f64) -> Project {
        let mut p = Project::default();
        let output = &mut p.main.mixer_device.volume.output;
        output.enabled = true;
        output.send_mode = SendMode::EveryFrame;
        output.stream_rate = hz;
        p
    }

    #[test]
    fn specs_hold_the_designed_ranges_and_defaults() {
        for (spec, min, max, default, unit) in [
            (FRAME_RATE, 1.0, 60.0, 30.0, " Hz"),
            (BUDGET, 10.0, 10_000.0, 1000.0, " msg/s"),
            (REFRESH, 0.1, 10.0, 1.0, " s"),
        ] {
            assert_eq!((spec.min, spec.max, spec.default), (min, max, default));
            assert_eq!(spec.unit.suffix(), unit);
        }
        assert_eq!(BUDGET.parse("600.4").unwrap(), 600.0);
        assert!(BUDGET.parse("9").is_err());
        assert!(REFRESH.parse("0.05").is_err());
        assert!(FRAME_RATE.parse("61").is_err());
        assert_eq!(
            BUDGET.parse("x").unwrap_err(),
            "Enter a valid budget in messages per second"
        );
    }

    #[test]
    fn stage_validation_follows_the_specs() {
        OutputStage::default().validate().unwrap();
        let valid = OutputStage {
            frame_rate: 60.0,
            budget_on: true,
            budget: 10_000,
            bundle: true,
            refresh_on: true,
            refresh: 0.1,
        };
        valid.validate().unwrap();
        for invalid in [
            OutputStage {
                frame_rate: 0.5,
                ..OutputStage::default()
            },
            OutputStage {
                frame_rate: f64::NAN,
                ..OutputStage::default()
            },
            // Held values are checked while Off too.
            OutputStage {
                budget: 9,
                ..OutputStage::default()
            },
            OutputStage {
                budget_on: true,
                budget: 10_001,
                ..OutputStage::default()
            },
            OutputStage {
                refresh: 10.5,
                ..OutputStage::default()
            },
            OutputStage {
                refresh_on: true,
                refresh: f64::INFINITY,
                ..OutputStage::default()
            },
        ] {
            assert!(invalid.validate().is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn off_fields_hold_their_typed_value_disabled() {
        let stage = OutputStage::default();
        assert_eq!((stage.budget(), stage.refresh()), (None, None));
        assert_eq!(stage.budget_parameter().value, BUDGET.default);
        assert!(!stage.budget_parameter().is_enabled);
        assert_eq!(stage.refresh_parameter().display(), "1.00");
        assert!(!stage.refresh_parameter().is_enabled);
        let on = OutputStage {
            budget_on: true,
            budget: 600,
            refresh_on: true,
            refresh: 0.8,
            ..stage
        };
        assert_eq!((on.budget(), on.refresh()), (Some(600), Some(0.8)));
        assert_eq!(on.budget_parameter().display(), "600");
        assert!(on.budget_parameter().is_enabled && on.refresh_parameter().is_enabled);
        assert_eq!(on.frame_rate_parameter().display(), "30.00");
        // Switching Off keeps the typed values in the fields, ready to switch back on.
        let off = OutputStage {
            budget_on: false,
            refresh_on: false,
            ..on.clone()
        };
        assert_eq!((off.budget(), off.refresh()), (None, None));
        assert_eq!(off.budget_parameter().display(), "600");
        assert_eq!(off.refresh_parameter().display(), "0.80");
        assert!(!off.budget_parameter().is_enabled && !off.refresh_parameter().is_enabled);
        assert_eq!(
            OutputStage {
                budget_on: true,
                refresh_on: true,
                ..off
            },
            on
        );
    }

    #[test]
    fn project_validation_checks_output_and_main_stages() {
        let mut p = Project::default();
        p.outputs
            .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9000));
        p.validate().unwrap();
        p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 61.0;
        assert!(p.validate().is_err());
        assert!(p.validate_stored().is_err());
        p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 44.0;
        p.main.output.as_mut().unwrap().stage.budget = 5;
        assert!(p.validate().is_err());
        p.main.output.as_mut().unwrap().stage.budget = 500;
        p.validate().unwrap();
    }

    #[test]
    fn notice_only_when_an_every_frame_rate_differs_from_a_frame_rate() {
        assert_eq!(Project::default().every_frame_rate_notice(), None);
        assert_eq!(streaming(30.0).every_frame_rate_notice(), None);
        assert_eq!(
            streaming(50.0).every_frame_rate_notice(),
            Some(EVERY_FRAME_RATE_NOTICE)
        );
        let mut matched = streaming(50.0);
        matched.main.output.as_mut().unwrap().stage.frame_rate = 50.0;
        assert_eq!(matched.every_frame_rate_notice(), None);
        let mut other_mode = streaming(50.0);
        other_mode.main.mixer_device.volume.output.send_mode = SendMode::Continuous;
        assert_eq!(other_mode.every_frame_rate_notice(), None);
        let mut disabled = streaming(50.0);
        disabled.main.mixer_device.volume.output.enabled = false;
        assert_eq!(disabled.every_frame_rate_notice(), None);
    }

    #[test]
    fn notice_compares_a_track_against_each_routed_output() {
        let mut p = Project::default();
        let mut fast = Track::new_output(TrackId(201), "Fast", "127.0.0.1", 9000);
        fast.output.as_mut().unwrap().stage.frame_rate = 60.0;
        p.outputs.push(fast);
        let mut track = Track::new(TrackId(1), "Kick", p.scenes.len());
        track.route = Route::Outputs(vec![p.main.id, TrackId(201)]);
        let output = &mut track.mixer_device.volume.output;
        output.enabled = true;
        output.send_mode = SendMode::EveryFrame;
        output.stream_rate = 30.0;
        p.tracks.push(track);
        p.validate().unwrap();
        assert_eq!(p.every_frame_rate_notice(), Some(EVERY_FRAME_RATE_NOTICE));
        p.tracks[0].route = Route::single(p.main.id);
        assert_eq!(p.every_frame_rate_notice(), None);
    }
}
