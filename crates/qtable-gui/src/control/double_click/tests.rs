use super::*;

fn at(x: f32) -> Point {
    Point::new(x, 5.0)
}

#[test]
fn two_presses_close_in_time_and_place_make_a_double_click_and_a_third_starts_over() {
    let start = Instant::now();
    let mut presses = Presses::default();
    assert!(!presses.register_at(at(10.0), start));
    assert!(presses.register_at(at(12.0), start + Duration::from_millis(200)));
    // The pair is spent: the next press is a first press again.
    assert!(!presses.register_at(at(12.0), start + Duration::from_millis(300)));
    assert!(presses.register_at(at(12.0), start + Duration::from_millis(400)));
}

#[test]
fn slow_or_distant_presses_do_not_chain() {
    let start = Instant::now();
    let mut presses = Presses::default();
    assert!(!presses.register_at(at(10.0), start));
    assert!(!presses.register_at(at(10.0), start + Duration::from_millis(301)));
    assert!(!presses.register_at(at(40.0), start + Duration::from_millis(400)));
    // A same-instant repeat is not a later press either.
    assert!(!presses.register_at(at(40.0), start + Duration::from_millis(400)));
}
