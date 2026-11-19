//! Transport-position readout in Live's bars.beats.sixteenths form, at a fixed width.
use qtable_engine::model::{TICKS_PER_BEAT, TimeSignature};

/// Width of the readout box. The box, not its text, sets the width, so the
/// controls to its right never move as the position changes.
pub(super) const READOUT_WIDTH: f32 = 92.0;

/// "  12.  3.  4" style text: right-aligned fields in a monospace font keep a
/// constant character count for any position below 10 000 bars.
pub(super) fn format_position(ticks: u64, signature: TimeSignature) -> String {
    let bar_ticks = signature.bar_length().ticks().max(1);
    let beat_ticks = (TICKS_PER_BEAT * 4 / u64::from(signature.denominator())).max(1);
    let sixteenth_ticks = (TICKS_PER_BEAT / 4).max(1);
    let within_bar = ticks % bar_ticks;
    let bar = ticks / bar_ticks + 1;
    let beat = within_bar / beat_ticks + 1;
    let sixteenth = within_bar % beat_ticks / sixteenth_ticks + 1;
    format!("{bar:>4}.{beat:>2}.{sixteenth:>2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn four_four() -> TimeSignature {
        TimeSignature::new(4, 4).unwrap()
    }

    #[test]
    fn zero_is_one_one_one() {
        assert_eq!(format_position(0, four_four()).replace(' ', ""), "1.1.1");
    }

    #[test]
    fn counts_bars_beats_and_sixteenths() {
        let sixteenth = TICKS_PER_BEAT / 4;
        let ticks = 3 * 4 * TICKS_PER_BEAT + 2 * TICKS_PER_BEAT + 3 * sixteenth;
        assert_eq!(
            format_position(ticks, four_four()).replace(' ', ""),
            "4.3.4"
        );
    }

    #[test]
    fn eighth_note_meters_count_eighth_beats() {
        let signature = TimeSignature::new(6, 8).unwrap();
        let beat = TICKS_PER_BEAT / 2;
        assert_eq!(
            format_position(5 * beat, signature).replace(' ', ""),
            "1.6.1"
        );
        assert_eq!(
            format_position(6 * beat, signature).replace(' ', ""),
            "2.1.1"
        );
    }

    #[test]
    fn readout_text_length_does_not_depend_on_the_position() {
        let signature = four_four();
        let width = format_position(0, signature).chars().count();
        for ticks in [
            1,
            TICKS_PER_BEAT * 3,
            TICKS_PER_BEAT * 4 * 9,
            TICKS_PER_BEAT * 4 * 99 + 777,
            TICKS_PER_BEAT * 4 * 9_998,
        ] {
            assert_eq!(format_position(ticks, signature).chars().count(), width);
        }
    }
}
