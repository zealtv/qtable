//! Content markers shared by clips and unlinked envelope lanes.
use super::*;

/// Live's start/end markers and independent loop brace, in content beats.
/// Kept in this module to distinguish it from the Session column `model::Region`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub start: BeatTime,
    pub end: BeatTime,
    pub loop_start: BeatTime,
    pub loop_end: BeatTime,
    pub looping: bool,
}
impl Region {
    /// Live accepts Start beyond the loop brace and wraps it by whole loop lengths.
    /// Invalid loop intervals stay unchanged and are rejected by validation.
    pub fn normalised(mut self) -> Self {
        if self.looping && self.start >= self.loop_end && self.loop_start < self.loop_end {
            self.start = BeatTime(
                self.loop_start.0
                    + (self.start.0 - self.loop_start.0) % (self.loop_end.0 - self.loop_start.0),
            );
        }
        self
    }

    pub fn length(self) -> BeatTime {
        if self.looping {
            BeatTime(self.loop_end.0.saturating_sub(self.loop_start.0))
        } else {
            BeatTime(self.end.0.saturating_sub(self.start.0))
        }
    }

    pub fn validate(self) -> ModelResult {
        require(self.start < self.end, "region start must be before end")?;
        require(
            self.loop_start < self.loop_end,
            "loop start must be before loop end",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> Region {
        Region {
            start: BeatTime(960),
            end: BeatTime(12 * 960),
            loop_start: BeatTime(4 * 960),
            loop_end: BeatTime(8 * 960),
            looping: true,
        }
    }

    #[test]
    fn independent_markers_derive_length_in_both_modes() {
        let mut r = region();
        r.validate().unwrap();
        assert_eq!(r.length(), BeatTime(4 * 960));
        r.looping = false;
        assert_eq!(r.length(), BeatTime(11 * 960));
        // A stored Start past the brace is valid. The later marker edit normalises it.
        r.start = BeatTime(9 * 960);
        r.looping = true;
        r.validate().unwrap();
        for looping in [false, true] {
            r.looping = looping;
            assert!(Region { start: r.end, ..r }.validate().is_err());
            assert!(
                Region {
                    start: BeatTime(r.end.0 + 1),
                    ..r
                }
                .validate()
                .is_err()
            );
            assert!(
                Region {
                    loop_start: r.loop_end,
                    ..r
                }
                .validate()
                .is_err()
            );
            assert!(
                Region {
                    loop_start: BeatTime(r.loop_end.0 + 1),
                    ..r
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn unknown_region_and_clip_fields_fail_instead_of_losing_data() {
        let r = ron::to_string(&region()).unwrap();
        assert!(ron::from_str::<Region>(&r.replacen("start:", "future:1,start:", 1)).is_err());
        let clip = Clip::new(ClipId(1), "clip", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
        let c = ron::to_string(&clip).unwrap();
        assert!(ron::from_str::<Clip>(&c.replacen("start:", "future:1,start:", 1)).is_err());
    }
}
