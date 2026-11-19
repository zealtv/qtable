//! Logical keys supplied by a host; auto-repeat is never another press.
use crate::Gesture;
use std::collections::HashSet;

#[derive(Default)]
pub struct Decoder {
    held: HashSet<String>,
}

impl Decoder {
    pub fn decode(&mut self, key: &str, pressed: bool, repeat: bool) -> Option<Gesture> {
        if pressed {
            if repeat || !self.held.insert(key.to_owned()) {
                return None;
            }
            Some(Gesture::Press)
        } else {
            self.held.remove(key).then_some(Gesture::Release)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_and_duplicate_press_are_suppressed_until_release() {
        let mut decoder = Decoder::default();
        assert_eq!(decoder.decode("a", true, false), Some(Gesture::Press));
        assert_eq!(decoder.decode("a", true, true), None);
        assert_eq!(decoder.decode("a", true, false), None);
        assert_eq!(decoder.decode("a", false, false), Some(Gesture::Release));
        assert_eq!(decoder.decode("a", true, false), Some(Gesture::Press));
    }
}
