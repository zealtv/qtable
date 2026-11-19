//! Track-local clip automation, in argument control units.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentRef {
    pub device: DeviceRef,
    pub parameter: ParameterKey,
    pub field: FieldKey,
}
impl ArgumentRef {
    /// Mixer Volume's one argument.
    pub fn volume() -> Self {
        Self {
            device: DeviceRef::Mixer,
            parameter: ParameterKey(VOLUME_PARAMETER.into()),
            field: FieldKey(VOLUME_FIELD.into()),
        }
    }

    /// Whether this is Mixer Volume's argument.
    pub fn is_volume(&self) -> bool {
        self.device == DeviceRef::Mixer
            && self.parameter.0 == VOLUME_PARAMETER
            && self.field.0 == VOLUME_FIELD
    }

    pub fn on(&self, track: TrackId) -> FieldRef {
        FieldRef {
            parameter: ParameterRef {
                track,
                device: self.device,
                key: self.parameter.clone(),
            },
            field: self.field.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Continuous,
    Stepped,
    Discrete,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Breakpoint {
    pub time: BeatTime,
    pub value: f64,
    #[serde(default, skip_serializing_if = "zero")]
    pub curve: f32,
}
fn zero(value: &f32) -> bool {
    *value == 0.0
}
impl Breakpoint {
    /// The line's value at `ticks` on the segment from this point to `next`, by this point's
    /// curve; held to the segment's ends.
    pub fn towards(&self, next: &Breakpoint, ticks: f64) -> f64 {
        let f = ((ticks - self.time.ticks() as f64)
            / (next.time.ticks() - self.time.ticks()) as f64)
            .clamp(0.0, 1.0);
        self.value + (next.value - self.value) * f.powf(2.0_f64.powf(4.0 * f64::from(self.curve)))
    }
}

/// The curve for the part of a segment after `fraction` of it, where the segment has `curve`.
/// The part before keeps `curve` exactly (a power curve cut short is the same power curve).
/// The part after is no power curve, so this picks the curve that keeps it closest: within 3%
/// of the segment's value range at the extreme curves, 2.2% for |curve| <= 0.5
/// (`curve_after_stays_close_to_the_cut_segment`).
pub fn curve_after(curve: f32, fraction: f64) -> f32 {
    let k = 2.0_f64.powf(4.0 * f64::from(curve));
    let fraction = fraction.clamp(0.0, 1.0);
    let start = fraction.powf(k);
    let span = 1.0 - start;
    if span <= 1e-9 {
        return 0.0;
    }
    if curve == 0.0 || fraction == 0.0 {
        return curve;
    }
    // The worst gap between the rest and a candidate, over 33 samples.
    let gap = |candidate: f64| {
        let k2 = 2.0_f64.powf(4.0 * candidate);
        (0..=32)
            .map(|i| {
                let g = f64::from(i) / 32.0;
                let rest = ((fraction + (1.0 - fraction) * g).powf(k) - start) / span;
                (rest - g.powf(k2)).abs()
            })
            .fold(0.0, f64::max)
    };
    let best = |candidates: &mut dyn Iterator<Item = f64>| {
        candidates
            .map(|c| (gap(c), c))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(0.0, |(_, c)| c)
    };
    let coarse = best(&mut (-20..=20).map(|i| f64::from(i) / 20.0));
    let fine = best(
        &mut (-40..=40)
            .map(|i| coarse + f64::from(i) / 800.0)
            .filter(|c| (-1.0..=1.0).contains(c)),
    );
    fine as f32
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationEnvelope {
    pub target: ArgumentRef,
    pub points: Vec<Breakpoint>,
    /// None follows the clip; Some carries an independent envelope timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<region::Region>,
}
impl AutomationEnvelope {
    /// Fractional ticks keep frame-time sampling independent of the editing grid.
    pub fn value_at(&self, ticks: f64, axis: Axis) -> Option<f64> {
        let first = self.points.first()?;
        let right = self
            .points
            .partition_point(|p| p.time.ticks() as f64 <= ticks);
        let value = if right == 0 {
            first.value
        } else {
            let left = &self.points[right - 1];
            match self.points.get(right) {
                Some(next) if axis != Axis::Discrete => left.towards(next, ticks),
                _ => left.value,
            }
        };
        Some(if axis == Axis::Stepped {
            value.round()
        } else {
            value
        })
    }
    pub fn validate(&self) -> ModelResult {
        if let Some(region) = self.region {
            region.validate()?;
        }
        Self::validate_points(&self.points)
    }

    pub(crate) fn validate_points(points: &[Breakpoint]) -> ModelResult {
        for (i, p) in points.iter().enumerate() {
            require(p.value.is_finite(), "envelope value must be finite")?;
            require(
                p.curve.is_finite() && (-1.0..=1.0).contains(&p.curve),
                "envelope curvature must be in -1..1",
            )?;
            require(
                i == 0 || points[i - 1].time <= p.time,
                "envelope points must be sorted",
            )?;
            require(
                i < 2 || points[i - 2].time != p.time,
                "at most two envelope points may share a time",
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Lane {
    Events(EventLane),
    Envelope(AutomationEnvelope),
}
impl Lane {
    pub fn event(&self) -> Option<&EventLane> {
        match self {
            Self::Events(lane) => Some(lane),
            _ => None,
        }
    }
    pub fn event_mut(&mut self) -> Option<&mut EventLane> {
        match self {
            Self::Events(lane) => Some(lane),
            _ => None,
        }
    }
    pub fn envelope(&self) -> Option<&AutomationEnvelope> {
        match self {
            Self::Envelope(lane) => Some(lane),
            _ => None,
        }
    }
}
impl From<EventLane> for Lane {
    fn from(lane: EventLane) -> Self {
        Self::Events(lane)
    }
}

impl Track {
    /// Number of clips and points using this argument, including empty envelope lanes.
    pub fn field_uses(&self, target: &ArgumentRef) -> (usize, usize) {
        self.slots
            .iter()
            .filter_map(|s| s.clip.as_ref())
            .flat_map(|c| c.lanes.iter().filter_map(Lane::envelope))
            .filter(|e| e.target == *target)
            .fold((0, 0), |(clips, points), e| {
                (clips + 1, points + e.points.len())
            })
    }

    pub fn argument_axis(&self, target: &ArgumentRef) -> Option<Axis> {
        match target.device {
            DeviceRef::Mixer if target.is_volume() => Some(Axis::Continuous),
            DeviceRef::Chain(_) => Some(self.argument(target)?.2.axis()),
            _ => None,
        }
    }

    pub fn argument_spec(&self, target: &ArgumentRef) -> Option<ParameterSpec> {
        match target.device {
            DeviceRef::Mixer if target.is_volume() => Some(VOLUME.clone()),
            DeviceRef::Chain(_) => {
                let (_, parameter, _) = self.argument(target)?;
                parameter.field_spec(
                    parameter
                        .fields
                        .iter()
                        .position(|f| f.key == target.field)?,
                )
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_after_stays_close_to_the_cut_segment() {
        // A 0 -> 1 segment cut at `cut`: the fitted rest against the true curve, as a share
        // of the whole segment's value range.
        let mut worst: f64 = 0.0;
        for c in -10..=10 {
            let curve = c as f32 / 10.0;
            let whole = envelope(&[(0, 0.0, curve), (1000, 1.0, 0.0)]);
            for cut in (50..1000).step_by(50) {
                let at = whole.value_at(cut as f64, Axis::Continuous).unwrap();
                let rest = envelope(&[
                    (cut, at, curve_after(curve, cut as f64 / 1000.0)),
                    (1000, 1.0, 0.0),
                ]);
                for t in (cut..=1000).step_by(10) {
                    let error = (rest.value_at(t as f64, Axis::Continuous).unwrap()
                        - whole.value_at(t as f64, Axis::Continuous).unwrap())
                    .abs();
                    worst = worst.max(error);
                }
            }
        }
        assert!(worst < 0.03, "{worst}");
        // An uncut or straight segment keeps its curve.
        assert_eq!(curve_after(0.6, 0.0), 0.6);
        assert_eq!(curve_after(0.0, 0.5), 0.0);
    }
    fn envelope(points: &[(u64, f64, f32)]) -> AutomationEnvelope {
        AutomationEnvelope {
            region: None,
            target: ArgumentRef::volume(),
            points: points
                .iter()
                .map(|&(t, value, curve)| Breakpoint {
                    time: BeatTime(t),
                    value,
                    curve,
                })
                .collect(),
        }
    }
    #[test]
    fn interpolation_extrapolation_jumps_and_axes() {
        let e = envelope(&[
            (100, 0.0, 0.0),
            (200, 1.0, 0.0),
            (200, 0.25, 0.0),
            (300, 0.75, 0.0),
        ]);
        e.validate().unwrap();
        for (t, expected) in [
            (0.0, 0.0),
            (150.0, 0.5),
            (199.0, 0.99),
            (200.0, 0.25),
            (250.0, 0.5),
            (400.0, 0.75),
        ] {
            assert_eq!(e.value_at(t, Axis::Continuous), Some(expected));
        }
        assert_eq!(e.value_at(175.0, Axis::Discrete), Some(0.0));
        assert_eq!(e.value_at(175.0, Axis::Stepped), Some(1.0));
        assert_eq!(envelope(&[]).value_at(0.0, Axis::Continuous), None);
        let curved = envelope(&[(0, 0.0, 1.0), (100, 1.0, 0.0)]);
        assert_eq!(
            curved.value_at(50.0, Axis::Continuous),
            Some(0.5_f64.powi(16))
        );
        let fast = envelope(&[(0, 0.0, -1.0), (100, 1.0, 0.0)]);
        assert_eq!(
            fast.value_at(50.0, Axis::Continuous),
            Some(0.5_f64.powf(1.0 / 16.0))
        );
    }
    #[test]
    fn malformed_points_are_rejected() {
        for points in [
            vec![(2, 0.0, 0.0), (1, 1.0, 0.0)],
            vec![(0, 0.0, 0.0); 3],
            vec![(0, f64::NAN, 0.0)],
            vec![(0, 0.0, f32::INFINITY)],
            vec![(0, 0.0, 1.1)],
        ] {
            assert!(envelope(&points).validate().is_err());
        }
    }
    #[test]
    fn unknown_fields_cannot_be_silently_lost() {
        assert!(ron::from_str::<Breakpoint>("(time: 0.0, value: 0.5, future: 1)").is_err());
        let e = envelope(&[(0, 0.0, 0.0)]);
        let source = ron::to_string(&e).unwrap();
        let bad = source.replacen("points:", "future:1,points:", 1);
        assert!(ron::from_str::<AutomationEnvelope>(&bad).is_err());
        assert!(ron::from_str::<Lane>(&format!("Envelope({bad})")).is_err());
    }
}
