//! Exact beat display and fraction input on the engine's tick grid.
use qtable_engine::model::{BeatTime, TICKS_PER_BEAT};

const NUMBER_ERROR: &str = "Enter non-negative beats: a number, 1/3, or 2+1/3";
const RANGE_ERROR: &str = "Beat value is too large";

#[cfg(test)]
pub(super) fn format_beats(time: BeatTime) -> String {
    let whole = time.ticks() / TICKS_PER_BEAT;
    let remainder = time.ticks() % TICKS_PER_BEAT;
    if remainder == 0 {
        return whole.to_string();
    }
    let divisor = gcd(remainder, TICKS_PER_BEAT);
    let numerator = remainder / divisor;
    let denominator = TICKS_PER_BEAT / divisor;
    // With 960 ticks/beat, terminating decimals need at most six places.
    const SCALE: u64 = 1_000_000;
    if SCALE.is_multiple_of(denominator) {
        return format!("{whole}.{:06}", numerator * (SCALE / denominator))
            .trim_end_matches('0')
            .to_owned();
    }
    if whole == 0 {
        format!("{numerator}/{denominator}")
    } else {
        format!("{whole}+{numerator}/{denominator}")
    }
}

pub(crate) fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

pub fn parse_beats(input: &str) -> Result<BeatTime, String> {
    let input = input.trim();
    if input.contains('/') {
        let (whole, fraction) = match input.split_once('+') {
            Some((whole, fraction)) => (parse_integer(whole)?, fraction.trim()),
            None => (0, input),
        };
        let (numerator, denominator) = fraction.split_once('/').ok_or(NUMBER_ERROR)?;
        let numerator = u128::from(parse_integer(numerator)?);
        let denominator = u128::from(parse_integer(denominator)?);
        if denominator == 0 {
            return Err("Fraction denominator must be positive".into());
        }
        let ticks = numerator * u128::from(TICKS_PER_BEAT);
        if !ticks.is_multiple_of(denominator) {
            return Err(format!(
                "Fraction must fall on the 1/{TICKS_PER_BEAT} beat grid"
            ));
        }
        return combine_ticks(whole, ticks / denominator);
    }

    let value = input.parse::<f64>().map_err(|_| NUMBER_ERROR)?;
    if !value.is_finite() || value < 0.0 {
        return Err(NUMBER_ERROR.into());
    }
    // Parse ordinary decimal notation with integers so even large formatted
    // values retain every tick. Scientific notation keeps its existing syntax.
    let unsigned = input.strip_prefix('+').unwrap_or(input);
    if !unsigned.contains(['e', 'E']) {
        let (whole, fractional) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let whole = if whole.is_empty() {
            0
        } else {
            parse_integer(whole)?
        };
        if fractional.len() <= 35 {
            let numerator = if fractional.is_empty() {
                0
            } else {
                fractional.parse::<u128>().map_err(|_| NUMBER_ERROR)?
            };
            // 10^35 * 960 and the rounding adjustment both fit in u128.
            let denominator = 10_u128.pow(fractional.len() as u32);
            let ticks = (numerator * u128::from(TICKS_PER_BEAT) + denominator / 2) / denominator;
            return combine_ticks(whole, ticks);
        }
    }
    let ticks = (value * TICKS_PER_BEAT as f64).round();
    // u64::MAX rounds up to 2^64 in f64, so reject that boundary rather than
    // relying on float-to-integer saturation. Displayed values use the exact path.
    if !ticks.is_finite() || ticks >= u64::MAX as f64 {
        return Err(RANGE_ERROR.into());
    }
    Ok(BeatTime(ticks as u64))
}

fn parse_integer(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(NUMBER_ERROR.into());
    }
    input.parse::<u64>().map_err(|_| NUMBER_ERROR.into())
}

fn combine_ticks(whole: u64, fractional_ticks: u128) -> Result<BeatTime, String> {
    let ticks = u128::from(whole) * u128::from(TICKS_PER_BEAT) + fractional_ticks;
    u64::try_from(ticks)
        .map(BeatTime)
        .map_err(|_| RANGE_ERROR.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tick_in_four_beats_round_trips() {
        for ticks in 0..=3840 {
            let time = BeatTime(ticks);
            let formatted = format_beats(time);
            assert_eq!(
                parse_beats(&formatted),
                Ok(time),
                "tick {ticks}: {formatted}"
            );
        }
    }

    #[test]
    fn whole_beats_and_terminating_decimals_stay_readable() {
        for (ticks, expected) in [
            (0, "0"),
            (960, "1"),
            (1440, "1.5"),
            (15, "0.015625"),
            (6, "0.00625"),
        ] {
            assert_eq!(format_beats(BeatTime(ticks)), expected);
        }
        assert_eq!(format_beats(BeatTime(320)), "1/3");
        assert_eq!(format_beats(BeatTime(2240)), "2+1/3");
        assert_eq!(format_beats(BeatTime(1)), "1/960");
    }

    #[test]
    fn fractions_and_mixed_fractions_author_triplets_exactly() {
        for (input, ticks) in [
            ("1/3", 320),
            ("2/3", 640),
            ("2+1/3", 2240),
            (" 2 + 1 / 3 ", 2240),
            ("5/3", 1600),
            ("1/960", 1),
            ("0/3", 0),
        ] {
            assert_eq!(parse_beats(input), Ok(BeatTime(ticks)), "{input}");
        }
    }

    #[test]
    fn decimals_round_to_the_nearest_tick_including_approximate_triplets() {
        for (input, ticks) in [
            ("0.333", 320),
            ("0.667", 640),
            ("0.001", 1),
            (".5", 480),
            ("1.", 960),
            ("+1.5", 1440),
            ("1e-3", 1),
            ("0.0005", 0),
            ("0.0006", 1),
            ("0.0015625", 2),
            ("0.015625", 15),
        ] {
            assert_eq!(parse_beats(input), Ok(BeatTime(ticks)), "{input}");
        }
    }

    #[test]
    fn full_tick_range_round_trips_and_overflow_is_rejected() {
        let near_max = (u64::MAX - TICKS_PER_BEAT)..=u64::MAX;
        for ticks in near_max.chain([(1 << 53) + 1, u64::MAX / 2]) {
            let time = BeatTime(ticks);
            assert_eq!(parse_beats(&format_beats(time)), Ok(time));
        }
        for input in [
            "18446744073709551615",
            "18446744073709551615/1",
            "18446744073709551615+1/3",
            "1e100",
            "1e309",
        ] {
            assert!(parse_beats(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn malformed_nonfinite_negative_and_off_grid_fractions_are_rejected() {
        for input in [
            "", "beats", "-1", "NaN", "inf", "-inf", "1/0", "1/7", "1/-3", "-1/3", "1//3", "1/3+2",
            "2+", "2+1/3/4", "1.5/3", "1+2+1/3", "2++1/3", "1..5",
        ] {
            assert!(parse_beats(input).is_err(), "accepted {input}");
        }
    }
}
