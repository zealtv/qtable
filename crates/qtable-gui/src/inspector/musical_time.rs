//! Inspector-facing musical time helpers; the shared control owns time syntax.
pub(crate) use crate::control::time_field::{
    format_duration, format_position, parse_duration, parse_position,
};
use qtable_engine::model::FollowMultiplier;

/// Live shows the Linked multiplier as loops, e.g. "1x" or "1/2x".
pub(super) fn format_multiplier(multiplier: FollowMultiplier) -> String {
    match multiplier.denominator() {
        1 => format!("{}x", multiplier.numerator()),
        d => format!("{}/{d}x", multiplier.numerator()),
    }
}

/// Accepts `2`, `2x`, `1/2` and short decimals such as `0.5`.
pub(crate) fn parse_multiplier(input: &str) -> Result<FollowMultiplier, String> {
    let error = || "Enter a positive number of loops, e.g. 2x, 1/2 or 0.5".to_owned();
    let text = input.trim().trim_end_matches(['x', 'X']).trim();
    let (numerator, denominator) = if let Some((n, d)) = text.split_once('/') {
        (
            n.trim().parse::<u32>().map_err(|_| error())?,
            d.trim().parse::<u32>().map_err(|_| error())?,
        )
    } else if text.contains('.') {
        let value: f64 = text.parse().map_err(|_| error())?;
        (1..=1000u32)
            .find_map(|d| {
                let n = value * f64::from(d);
                let rounded = n.round();
                ((n - rounded).abs() < 1e-9 && rounded >= 1.0 && rounded <= f64::from(u32::MAX))
                    .then_some((rounded as u32, d))
            })
            .ok_or_else(error)?
    } else {
        (text.parse::<u32>().map_err(|_| error())?, 1)
    };
    let divisor =
        crate::control::time_field::beats::gcd(u64::from(numerator), u64::from(denominator)).max(1)
            as u32;
    FollowMultiplier::new(numerator / divisor, denominator / divisor).map_err(|_| error())
}

pub(crate) fn adjust_multiplier(text: &str, up: bool, fine: bool) -> Option<String> {
    let value = parse_multiplier(text).ok()?;
    let scale = if fine { 4u64 } else { 1 };
    let numerator = u64::from(value.numerator()) * scale;
    let denominator = u64::from(value.denominator()) * scale;
    let step = u64::from(value.denominator());
    let numerator = if up {
        numerator.checked_add(step)?
    } else {
        numerator.saturating_sub(step).max(1)
    };
    let divisor = crate::control::time_field::beats::gcd(numerator, denominator);
    Some(format_multiplier(
        FollowMultiplier::new(
            u32::try_from(numerator / divisor).ok()?,
            u32::try_from(denominator / divisor).ok()?,
        )
        .ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multipliers_round_trip_and_accept_live_style_input() {
        let m = |n, d| FollowMultiplier::new(n, d).unwrap();
        for (text, expected) in [
            ("2", m(2, 1)),
            (" 2x ", m(2, 1)),
            ("1/2", m(1, 2)),
            ("2/4x", m(1, 2)),
            ("0.5", m(1, 2)),
            ("1.25x", m(5, 4)),
        ] {
            assert_eq!(parse_multiplier(text), Ok(expected), "{text}");
        }
        for value in [m(1, 1), m(8, 1), m(1, 8), m(3, 2)] {
            assert_eq!(parse_multiplier(&format_multiplier(value)), Ok(value));
        }
        assert_eq!(format_multiplier(m(1, 1)), "1x");
        assert_eq!(format_multiplier(m(1, 2)), "1/2x");
        for bad in ["", "0", "0x", "1/0", "-1", "abc", "0.0001", "1/"] {
            assert!(parse_multiplier(bad).is_err(), "{bad}");
        }
    }
}
