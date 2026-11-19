//! Duration fields use the project meter; positions begin at 1.1.1.
use super::beats::parse_beats;
use qtable_engine::model::{BeatTime, TICKS_PER_BEAT, TimeSignature};

/// Blank components have the same defaults as omitted ones.
fn component<'a>(parts: &[&'a str], index: usize, default: &'a str) -> &'a str {
    parts
        .get(index)
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .unwrap_or(default)
}

/// A sixteenth is an unsigned integer, a fraction, or an integer plus a fraction.
/// Validate the grammar before reusing the exact beat-grid arithmetic (scaled by four).
fn parse_sixteenth(input: &str) -> Result<BeatTime, String> {
    let integer =
        |part: &str| !part.trim().is_empty() && part.trim().bytes().all(|b| b.is_ascii_digit());
    let fraction = |part: &str| {
        part.split_once('/')
            .is_some_and(|(a, b)| integer(a) && integer(b))
    };
    let valid = if let Some((whole, rest)) = input.split_once('+') {
        integer(whole) && fraction(rest)
    } else {
        integer(input) || fraction(input)
    };
    if !valid {
        return Err("Enter a sixteenth as n, a/b or n+a/b".into());
    }
    let ticks = parse_beats(input)?.ticks();
    if !ticks.is_multiple_of(4) {
        return Err("Fraction must fall on the tick grid".into());
    }
    Ok(BeatTime(ticks / 4))
}

pub(crate) fn format_duration(time: BeatTime, signature: TimeSignature) -> String {
    let bar = signature.bar_length().ticks();
    let beat = bar / u64::from(signature.numerator());
    let within_bar = time.ticks() % bar;
    // Fractions in the last component preserve events on the engine's exact tick grid.
    let remainder = within_bar % beat;
    let unit = TICKS_PER_BEAT / 4;
    let whole = remainder / unit;
    let fraction = remainder % unit;
    let sixteenths = if fraction == 0 {
        whole.to_string()
    } else {
        let divisor = super::beats::gcd(fraction, unit);
        let fraction = format!("{}/{}", fraction / divisor, unit / divisor);
        if whole == 0 {
            fraction
        } else {
            format!("{whole}+{fraction}")
        }
    };
    format!(
        "{}. {}. {}",
        time.ticks() / bar,
        within_bar / beat,
        sixteenths
    )
}

pub(crate) fn parse_duration(input: &str, signature: TimeSignature) -> Result<BeatTime, String> {
    let parts: Vec<_> = input.split('.').collect();
    let error = "Enter bars.beats.sixteenths, e.g. 1. 0. 0";
    if parts.is_empty() || parts.len() > 3 {
        return Err(error.into());
    }
    let bars = component(&parts, 0, "0")
        .parse::<u64>()
        .map_err(|_| error)?;
    let beats = component(&parts, 1, "0")
        .parse::<u64>()
        .map_err(|_| error)?;
    let last = parse_sixteenth(component(&parts, 2, "0"))?.ticks();
    let bar = signature.bar_length().ticks();
    let beat = bar / u64::from(signature.numerator());
    if beats >= u64::from(signature.numerator()) || last >= beat {
        return Err("Beat and sixteenth components must fit inside the project meter".into());
    }
    let ticks = u128::from(bars) * u128::from(bar)
        + u128::from(beats) * u128::from(beat)
        + u128::from(last);
    u64::try_from(ticks)
        .map(BeatTime)
        .map_err(|_| "Duration is too large".into())
}

/// A point's position as Live's 1.1.1 reads it: bars, beats and sixteenths all count from 1.
/// A time off the sixteenth grid keeps its fraction in the last part, as `format_duration`
/// does ("1. 1. 2+1/3").
pub(crate) fn format_position(time: BeatTime, signature: TimeSignature) -> String {
    let bar = signature.bar_length().ticks();
    let beat = bar / u64::from(signature.numerator());
    let within_bar = time.ticks() % bar;
    let remainder = within_bar % beat;
    let unit = TICKS_PER_BEAT / 4;
    let (whole, fraction) = (remainder / unit, remainder % unit);
    let sixteenths = if fraction == 0 {
        (whole + 1).to_string()
    } else {
        let divisor = super::beats::gcd(fraction, unit);
        format!("{}+{}/{}", whole + 1, fraction / divisor, unit / divisor)
    };
    format!(
        "{}. {}. {}",
        time.ticks() / bar + 1,
        within_bar / beat + 1,
        sixteenths
    )
}

/// Reads `format_position`'s form, and nothing before 1. 1. 1.
pub(crate) fn parse_position(input: &str, signature: TimeSignature) -> Result<BeatTime, String> {
    let error = "Enter a position as bars.beats.sixteenths, e.g. 2. 1. 1";
    let parts: Vec<_> = input.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(error.into());
    }
    let before = |part: &str| -> Result<u64, String> {
        part.trim()
            .parse::<u64>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .ok_or_else(|| error.to_owned())
    };
    let (bars, beats) = (
        before(component(&parts, 0, "1"))?,
        before(component(&parts, 1, "1"))?,
    );
    let last = parse_sixteenth(component(&parts, 2, "1"))?
        .ticks()
        .checked_sub(TICKS_PER_BEAT / 4)
        .ok_or(error)?;
    let bar = signature.bar_length().ticks();
    let beat = bar / u64::from(signature.numerator());
    if beats >= u64::from(signature.numerator()) || last >= beat {
        return Err("Beat and sixteenth components must fit inside the project meter".into());
    }
    let ticks = u128::from(bars) * u128::from(bar)
        + u128::from(beats) * u128::from(beat)
        + u128::from(last);
    u64::try_from(ticks)
        .map(BeatTime)
        .map_err(|_| "Position is too large".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations_round_trip_exact_ticks_and_non_four_four_meters() {
        for signature in [
            TimeSignature::default(),
            TimeSignature::new(7, 8).unwrap(),
            TimeSignature::new(3, 2).unwrap(),
            TimeSignature::new(5, 32).unwrap(),
        ] {
            for ticks in (0..8000).chain([u64::MAX]) {
                let time = BeatTime(ticks);
                assert_eq!(
                    parse_duration(&format_duration(time, signature), signature),
                    Ok(time)
                );
            }
        }
        assert_eq!(
            format_duration(BeatTime(15360), TimeSignature::default()),
            "4. 0. 0"
        );
        assert_eq!(
            format_duration(BeatTime(4320), TimeSignature::default()),
            "1. 0. 2"
        );
    }

    #[test]
    fn positions_count_from_one_and_round_trip_exact_ticks() {
        for signature in [
            TimeSignature::default(),
            TimeSignature::new(7, 8).unwrap(),
            TimeSignature::new(3, 2).unwrap(),
        ] {
            for ticks in (0..8000).chain([u64::MAX / 2]) {
                let time = BeatTime(ticks);
                assert_eq!(
                    parse_position(&format_position(time, signature), signature),
                    Ok(time),
                    "{ticks}"
                );
            }
        }
        let four_four = TimeSignature::default();
        assert_eq!(format_position(BeatTime(0), four_four), "1. 1. 1");
        assert_eq!(format_position(BeatTime(4320), four_four), "2. 1. 3");
        assert_eq!(parse_position("2. 1. 3", four_four), Ok(BeatTime(4320)));
        for bad in [
            "0. 1. 1",
            "1. 0. 1",
            "1. 1. 0",
            "1. 5. 1",
            "1. 1. 5",
            "x",
            "1. 1. 1+1/2x",
        ] {
            assert!(parse_position(bad, four_four).is_err(), "{bad}");
        }
    }

    #[test]
    fn invalid_components_and_overflow_are_rejected() {
        for input in [
            "-1.0.0",
            "0.4.0",
            "0.0.4",
            "0.0.1/7",
            "1.0.0.0",
            "18446744073709551615.0.0",
        ] {
            assert!(
                parse_duration(input, TimeSignature::default()).is_err(),
                "{input}"
            );
        }
    }
}
