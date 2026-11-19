//! MIDI wire values become gestures only after selecting a mapping's mode.
use crate::Gesture;
use qtable_engine::model::{MidiMessage, MidiSource, RelativeEncoding, SourceMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Message {
    pub source: MidiSource,
    value: u16,
}

pub fn decode(bytes: &[u8]) -> Option<Message> {
    let [status, first, second] = *bytes else {
        return None;
    };
    if first > 127 || second > 127 {
        return None;
    }
    let (message, value) = match status & 0xf0 {
        0x80 => (MidiMessage::Note(first), 0),
        0x90 => (MidiMessage::Note(first), u16::from(second != 0)),
        0xb0 => (MidiMessage::Cc(first), u16::from(second)),
        0xe0 => (
            MidiMessage::PitchBend,
            u16::from(first) | u16::from(second) << 7,
        ),
        _ => return None,
    };
    Some(Message {
        source: MidiSource {
            channel: (status & 0x0f) + 1,
            message,
        },
        value,
    })
}

impl Message {
    pub fn gesture(self, mode: SourceMode) -> Gesture {
        match self.source.message {
            MidiMessage::Note(_) if self.value == 0 => Gesture::Release,
            MidiMessage::Note(_) => Gesture::Press,
            MidiMessage::Cc(_) => match mode {
                // Gate belongs to notes and keys (validation rejects it on a CC); a stored Gate
                // on a CC reads as Absolute.
                SourceMode::Absolute | SourceMode::Gate => {
                    Gesture::Absolute(f64::from(self.value) / 127.0)
                }
                SourceMode::Relative(encoding) => {
                    Gesture::Steps(relative(self.value as u8, encoding))
                }
            },
            MidiMessage::PitchBend => Gesture::Absolute(f64::from(self.value) / 16383.0),
        }
    }
}

/// Live 12.1's measured curve, 2026-11-03.
/// Other encodings use the documented sign conventions; their curve is assumed.
pub fn relative(value: u8, encoding: RelativeEncoding) -> i32 {
    use RelativeEncoding::*;
    let value = i32::from(value);
    let steps = match encoding {
        SignedBit | SignedBitLinear => {
            if value <= 64 {
                value
            } else {
                64 - value
            }
        }
        SignedBit2 | SignedBit2Linear => {
            if value <= 64 {
                -value
            } else {
                value - 64
            }
        }
        BinOffset | BinOffsetLinear => value - 64,
        // The measured split overrides the manual's generic 64/65 convention.
        TwosComplement | TwosComplementLinear => {
            if value <= 65 {
                value
            } else {
                value - 128
            }
        }
    };
    if matches!(
        encoding,
        SignedBitLinear | SignedBit2Linear | BinOffsetLinear | TwosComplementLinear
    ) {
        return steps;
    }
    steps.signum()
        * match steps.abs() {
            0..=5 => steps.abs(),
            6 => 8,
            7 => 20,
            _ => 50,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_twos_complement_curve_for_every_wire_value() {
        for value in 0..=127 {
            let expected = match value {
                0..=5 => i32::from(value),
                6 => 8,
                7 => 20,
                8..=65 => 50,
                66..=120 => -50,
                121 => -20,
                122 => -8,
                _ => i32::from(value) - 128,
            };
            assert_eq!(
                relative(value, RelativeEncoding::TwosComplement),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn sign_conventions_and_linear_variants() {
        use RelativeEncoding::*;
        for (encoding, up, down, neutral) in [
            (SignedBit, 1, 65, 0),
            (SignedBit2, 65, 1, 0),
            (BinOffset, 65, 63, 64),
            (TwosComplement, 1, 127, 0),
        ] {
            assert_eq!(relative(up, encoding), 1);
            assert_eq!(relative(down, encoding), -1);
            assert_eq!(relative(neutral, encoding), 0);
        }
        for (encoding, value) in [
            (SignedBitLinear, 6),
            (SignedBit2Linear, 70),
            (BinOffsetLinear, 70),
            (TwosComplementLinear, 6),
        ] {
            assert_eq!(relative(value, encoding), 6);
        }
    }

    #[test]
    fn channels_absolute_scaling_note_off_and_bend() {
        let cc = decode(&[0xbf, 22, 64]).unwrap();
        assert_eq!(cc.source.channel, 16);
        assert_eq!(
            cc.gesture(SourceMode::Absolute),
            Gesture::Absolute(64.0 / 127.0)
        );
        for bytes in [[0x80, 60, 100], [0x90, 60, 0]] {
            assert_eq!(
                decode(&bytes).unwrap().gesture(SourceMode::Absolute),
                Gesture::Release
            );
        }
        assert_eq!(
            decode(&[0xe0, 127, 127])
                .unwrap()
                .gesture(SourceMode::Absolute),
            Gesture::Absolute(1.0)
        );
        for bytes in [
            &[0xb0, 1][..],
            &[0xb0, 128, 1],
            &[0xf0, 1, 2],
            &[0xb0, 1, 2, 3],
        ] {
            assert!(decode(bytes).is_none());
        }
    }
}
