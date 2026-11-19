//! Thinning envelope points: Ramer–Douglas–Peucker for sampled paths and for Simplify Envelope.
use super::{Axis, BeatTime, Breakpoint};
use std::ops::Range;

/// Ramer–Douglas–Peucker on a function of time: a sample goes when it lies within `tolerance`
/// (in value) of the line between the samples kept either side of it. The first and last
/// samples always stay.
pub fn thin(samples: &[(u64, f64)], tolerance: f64) -> Vec<(u64, f64)> {
    let n = samples.len();
    if n < 3 {
        return samples.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut spans = vec![(0, n - 1)];
    while let Some((a, b)) = spans.pop() {
        let ((ta, va), (tb, vb)) = (samples[a], samples[b]);
        let mut worst = (0.0, a);
        for (i, &(t, v)) in samples.iter().enumerate().take(b).skip(a + 1) {
            let chord = va + (vb - va) * (t - ta) as f64 / (tb - ta).max(1) as f64;
            let off = (v - chord).abs();
            if off > worst.0 {
                worst = (off, i);
            }
        }
        if worst.0 > tolerance {
            keep[worst.1] = true;
            spans.push((a, worst.1));
            spans.push((worst.1, b));
        }
    }
    samples
        .iter()
        .zip(keep)
        .filter_map(|(s, k)| k.then_some(*s))
        .collect()
}

/// How far a point may sit from the straight line between its neighbours and still go: the
/// larger of the argument's `threshold` and 1% of its range `min` to `max`. A whole-step
/// argument tolerates at most half a step, so no rounded value moves.
pub fn simplify_tolerance(threshold: f64, min: f64, max: f64, axis: Axis) -> f64 {
    let tolerance = threshold.max(0.01 * (max - min).abs());
    match axis {
        Axis::Continuous => tolerance,
        Axis::Stepped | Axis::Discrete => tolerance.min(0.5),
    }
}

/// `points` with every removable point strictly inside `scope` gone.
///
/// Never removed: a point outside `scope` (or on its edges), the first and last point, either
/// member of a jump pair (two points at one time) and either end of a curved segment. On a
/// continuous or whole-step argument those points cut the envelope into runs of straight
/// segments, and each run is thinned by [`thin`] with `tolerance`; the result never gains a
/// point and never changes a curved segment. A `Discrete` argument (a choice or switch) holds
/// its value between points, so a point goes when it repeats the value already held before it,
/// or when another point at the same time replaces it at once.
pub fn simplify_points(
    points: &[Breakpoint],
    scope: Range<BeatTime>,
    tolerance: f64,
    axis: Axis,
) -> Vec<Breakpoint> {
    let n = points.len();
    let inside =
        |i: usize| i > 0 && i + 1 < n && points[i].time > scope.start && points[i].time < scope.end;
    let mut keep = vec![true; n];
    if axis == Axis::Discrete {
        let mut held: Option<f64> = None;
        for i in 0..n {
            let replaced_at_once = points.get(i + 1).is_some_and(|p| p.time == points[i].time);
            if inside(i) && (replaced_at_once || held == Some(points[i].value)) {
                keep[i] = false;
            } else {
                held = Some(points[i].value);
            }
        }
    } else {
        let paired = |i: usize| {
            (i > 0 && points[i - 1].time == points[i].time)
                || points.get(i + 1).is_some_and(|p| p.time == points[i].time)
        };
        let curved = |i: usize| points[i].curve != 0.0 || (i > 0 && points[i - 1].curve != 0.0);
        let fixed: Vec<bool> = (0..n)
            .map(|i| !inside(i) || paired(i) || curved(i))
            .collect();
        let mut anchor = 0;
        for i in 1..n {
            if !fixed[i] {
                continue;
            }
            if i - anchor >= 2 {
                let samples: Vec<_> = points[anchor..=i]
                    .iter()
                    .map(|p| (p.time.ticks(), p.value))
                    .collect();
                let kept = thin(&samples, tolerance);
                for (offset, sample) in samples.iter().enumerate() {
                    if !kept.contains(sample) {
                        keep[anchor + offset] = false;
                    }
                }
            }
            anchor = i;
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| p.clone())
        .collect()
}

#[cfg(test)]
mod tests;
