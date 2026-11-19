use super::*;

/// Widths and extents the editor meets: a short clip in a narrow pane up to a long one wide.
const SPANS: [f32; 6] = [1.0, 48.0, 399.5, 768.0, 1223.25, 3000.0];
const EXTENTS: [u64; 6] = [1, 240, 3840, 7680, 61_440, 1_000_000];

/// The formulas every canvas used before the axis existed.
fn old_x(left: f32, span: f32, extent: u64, ticks: u64) -> f32 {
    left + (ticks as f64 / extent.max(1) as f64 * f64::from(span)) as f32
}
fn old_ticks_at(left: f32, span: f32, extent: u64, x: f32) -> f64 {
    ((f64::from(x - left) / f64::from(span)) * extent as f64).clamp(0.0, extent as f64)
}

#[test]
fn a_fitted_axis_maps_as_the_old_formulas_did() {
    for left in [0.0, 148.0, 52.5] {
        for span in SPANS {
            for extent in EXTENTS {
                let axis = TimeAxis::fitted(left, span, BeatTime(extent));
                for ticks in [
                    0,
                    1,
                    extent / 3,
                    extent / 2,
                    extent.saturating_sub(1),
                    extent,
                ] {
                    assert_eq!(
                        axis.x(BeatTime(ticks)),
                        old_x(left, span, extent, ticks),
                        "x of {ticks} in {extent} over {span} from {left}"
                    );
                }
                for step in 0..=20 {
                    let x = left - 10.0 + (span + 20.0) * step as f32 / 20.0;
                    assert_eq!(axis.ticks_at(x), old_ticks_at(left, span, extent, x));
                    assert_eq!(
                        axis.unclamped_ticks_at(x),
                        f64::from(x - left) / f64::from(span) * extent as f64
                    );
                }
                assert_eq!(axis.ticks_per_px(), extent as f64 / f64::from(span));
                assert_eq!(axis.visible_range(), (0.0, extent as f64));
                assert!(!axis.is_windowed());
            }
        }
    }
}

#[test]
fn a_fitted_drag_covers_the_ticks_the_old_formula_gave() {
    let axis = TimeAxis::fitted(148.0, 400.0, BeatTime(3840));
    for dx in [-37.5_f32, 0.0, 0.25, 50.0, 399.0] {
        assert_eq!(
            axis.delta_ticks(dx),
            f64::from(dx) / 400.0_f64 * 3840.0,
            "dx {dx}"
        );
    }
}

#[test]
fn a_windowed_axis_round_trips_x_and_ticks() {
    let window = Window {
        start: 960.0,
        visible: 480.0,
    };
    let axis = TimeAxis::windowed(100.0, 600.0, BeatTime(3840), window);
    assert!(axis.is_windowed());
    assert_eq!(axis.x(BeatTime(960)), 100.0);
    assert_eq!(axis.x(BeatTime(1440)), 700.0);
    assert_eq!(axis.x(BeatTime(1200)), 400.0);
    for ticks in [960_u64, 1000, 1200, 1337, 1440] {
        let back = axis.unclamped_ticks_at(axis.x(BeatTime(ticks)));
        assert!(
            (back - ticks as f64).abs() < 0.01,
            "{ticks} came back {back}"
        );
    }
    assert_eq!(axis.ticks_per_px(), 0.8);
    assert_eq!(axis.visible_range(), (960.0, 1440.0));
    // Before the window the x runs left of `left`; after it, right of the span.
    assert!(axis.x(BeatTime(0)) < 100.0);
    assert!(axis.x(BeatTime(3000)) > 700.0);
}

#[test]
fn ticks_at_clamps_to_the_extent_not_the_window() {
    let window = Window {
        start: 960.0,
        visible: 480.0,
    };
    let axis = TimeAxis::windowed(100.0, 600.0, BeatTime(3840), window);
    assert_eq!(axis.ticks_at(-5000.0), 0.0);
    assert_eq!(axis.ticks_at(50_000.0), 3840.0);
    assert_eq!(axis.ticks_at(400.0), 1200.0);
    // The unclamped reading still says how far past the content a pointer is.
    assert!(axis.unclamped_ticks_at(-5000.0) < 0.0);
    assert!(axis.unclamped_ticks_at(50_000.0) > 3840.0);
}

#[test]
fn a_window_is_held_inside_the_content_and_to_its_limits() {
    let extent = BeatTime(3840);
    let held = |start, visible| Window { start, visible }.clamped(extent);
    // Never wider than the content, and then it starts at 0 (fitted).
    assert_eq!(
        held(500.0, 9000.0),
        Window {
            start: 0.0,
            visible: 3840.0
        }
    );
    // Never narrower than a sixteenth note.
    assert_eq!(held(100.0, 10.0).visible, MIN_VISIBLE);
    // Slid back inside from either side.
    assert_eq!(held(-50.0, 480.0).start, 0.0);
    assert_eq!(held(3800.0, 480.0).start, 3840.0 - 480.0);
    // Non-finite input falls back to the whole content.
    assert_eq!(
        held(f64::NAN, f64::NAN),
        Window {
            start: 0.0,
            visible: 3840.0
        }
    );
    // Content shorter than a sixteenth stays whole.
    let tiny = Window {
        start: 0.0,
        visible: 5.0,
    }
    .clamped(BeatTime(100));
    assert_eq!(
        tiny,
        Window {
            start: 0.0,
            visible: 100.0
        }
    );
}

#[test]
fn a_window_over_the_whole_extent_maps_as_a_fitted_axis() {
    let extent = BeatTime(7680);
    let fitted = TimeAxis::fitted(148.0, 800.0, extent);
    let whole = TimeAxis::windowed(
        148.0,
        800.0,
        extent,
        Window {
            start: 0.0,
            visible: 7680.0,
        },
    );
    assert_eq!(fitted, whole);
    assert!(!whole.is_windowed());
}

#[test]
fn marks_show_only_inside_a_zoomed_window_and_always_on_a_fitted_axis() {
    let extent = BeatTime(3840);
    let fitted = TimeAxis::fitted(100.0, 600.0, extent);
    // A fitted axis draws everything, even a mark past the extent, as the canvases always did.
    for ticks in [0, 1920, 3840, 9000] {
        assert!(fitted.shows(BeatTime(ticks), 5.0));
    }
    let zoomed = TimeAxis::windowed(
        100.0,
        600.0,
        extent,
        Window {
            start: 960.0,
            visible: 480.0,
        },
    );
    assert!(zoomed.shows(BeatTime(1200), 5.0));
    assert!(zoomed.shows(BeatTime(960), 5.0));
    assert!(
        zoomed.shows(BeatTime(958), 5.0),
        "its half-width still reaches in"
    );
    assert!(!zoomed.shows(BeatTime(900), 5.0));
    assert!(!zoomed.shows(BeatTime(2000), 5.0));
}

const CLIP: BeatTime = BeatTime(15_360);

fn window(start: f64, visible: f64) -> Option<Window> {
    Some(Window { start, visible })
}

/// The tick under pixel `x` of a time area of `span` px showing `window`.
fn tick_at(window: Option<Window>, span: f32, x: f32) -> f64 {
    let w = Window::resolve(window, CLIP);
    w.start + f64::from(x) / f64::from(span) * w.visible
}

#[test]
fn dragging_down_zooms_in_and_keeps_the_pressed_time_under_the_pointer() {
    let span = 600.0;
    for from in [None, window(2000.0, 6000.0)] {
        for press in [0.0_f32, 150.0, 300.0, 599.0] {
            let grab = Grab::new(from, CLIP, f64::from(press) / f64::from(span));
            let before = tick_at(from, span, press);
            for dy in [10.0_f32, 50.0, 100.0, 140.0] {
                let zoomed = grab.at(CLIP, span, 0.0, dy);
                let now = Window::resolve(zoomed, CLIP);
                let was = Window::resolve(from, CLIP);
                assert!(now.visible < was.visible, "dy {dy} narrows the window");
                assert!(
                    (tick_at(zoomed, span, press) - before).abs() < 1e-6,
                    "press {press} dy {dy}: {} vs {before}",
                    tick_at(zoomed, span, press)
                );
            }
        }
    }
}

#[test]
fn a_drag_up_zooms_out_and_one_octave_of_travel_halves_the_window() {
    let from = window(4000.0, 4000.0);
    let grab = Grab::new(from, CLIP, 0.5);
    let doubled = Window::resolve(grab.at(CLIP, 600.0, 0.0, -ZOOM_OCTAVE_PX as f32), CLIP);
    assert!((doubled.visible - 8000.0).abs() < 1e-6);
    let halved = Window::resolve(grab.at(CLIP, 600.0, 0.0, ZOOM_OCTAVE_PX as f32), CLIP);
    assert!((halved.visible - 2000.0).abs() < 1e-6);
    assert!(
        (halved.start - 5000.0).abs() < 1e-6,
        "centre stays: {halved:?}"
    );
}

#[test]
fn dragging_sideways_scrolls_without_changing_the_scale() {
    let from = window(4000.0, 3000.0);
    let grab = Grab::new(from, CLIP, 0.25);
    let moved = Window::resolve(grab.at(CLIP, 600.0, -60.0, 0.0), CLIP);
    assert!((moved.visible - 3000.0).abs() < 1e-9);
    // The content follows the pointer: 60 px left is a tenth of the window later.
    assert!((moved.start - 4300.0).abs() < 1e-6, "{moved:?}");
    let back = Window::resolve(grab.at(CLIP, 600.0, 60.0, 0.0), CLIP);
    assert!((back.start - 3700.0).abs() < 1e-6, "{back:?}");
}

#[test]
fn both_at_once_zoom_and_scroll() {
    let grab = Grab::new(window(0.0, 8000.0), CLIP, 0.5);
    let moved = Window::resolve(grab.at(CLIP, 600.0, 30.0, 50.0), CLIP);
    assert!((moved.visible - 4000.0).abs() < 1e-6);
    // The pressed tick (4000) sits 30 px right of the middle, at 0.55 of the way.
    assert!(
        (moved.start + 0.55 * 4000.0 - 4000.0).abs() < 1e-6,
        "{moved:?}"
    );
}

#[test]
fn the_window_never_leaves_the_content_or_passes_either_limit() {
    let grab = Grab::new(window(1000.0, 2000.0), CLIP, 0.5);
    for (dx, dy) in [
        (-900.0, 0.0),
        (900.0, 0.0),
        (0.0, 900.0),
        (0.0, -900.0),
        (500.0, 500.0),
    ] {
        let w = Window::resolve(grab.at(CLIP, 600.0, dx, dy), CLIP);
        assert!(
            w.visible >= MIN_VISIBLE && w.visible <= CLIP.ticks() as f64,
            "{w:?}"
        );
        assert!(
            w.start >= 0.0 && w.start + w.visible <= CLIP.ticks() as f64 + 1e-9,
            "{w:?}"
        );
    }
    let narrowest = Window::resolve(grab.at(CLIP, 600.0, 0.0, 5000.0), CLIP);
    assert_eq!(narrowest.visible, MIN_VISIBLE);
    // A clip shorter than the limit is never narrowed past itself.
    let tiny = Grab::new(None, BeatTime(100), 0.5);
    assert_eq!(tiny.at(BeatTime(100), 600.0, 0.0, 500.0), None);
}

#[test]
fn zooming_all_the_way_out_returns_to_fitted() {
    let grab = Grab::new(window(1000.0, 2000.0), CLIP, 0.5);
    assert_eq!(grab.at(CLIP, 600.0, 0.0, -2000.0), None);
    assert_eq!(
        Window::zoom_about(window(0.0, 7680.0), CLIP, 0.5, 0.5),
        None
    );
    assert_eq!(
        Window::scroll(None, CLIP, 500.0),
        None,
        "fitted cannot scroll"
    );
    assert_eq!(
        Window::normalised(
            Window {
                start: 0.0,
                visible: 15_360.0
            },
            CLIP
        ),
        None
    );
}

#[test]
fn a_wheel_zoom_keeps_the_time_under_the_pointer() {
    let span = 600.0;
    for fraction in [0.0, 0.3, 0.5, 1.0] {
        for factor in [1.2, 2.0, 4.0] {
            let from = window(3000.0, 6000.0);
            let x = (fraction * f64::from(span)) as f32;
            let before = tick_at(from, span, x);
            let zoomed = Window::zoom_about(from, CLIP, fraction, factor);
            assert!(
                (tick_at(zoomed, span, x) - before).abs() < 1e-6,
                "{fraction} {factor}"
            );
            assert!(
                (Window::resolve(zoomed, CLIP).visible - 6000.0 / factor).abs() < 1e-6,
                "{fraction} {factor}"
            );
        }
    }
}

#[test]
fn scrolling_moves_the_window_and_stops_at_the_content() {
    let from = window(3000.0, 3000.0);
    assert_eq!(Window::scroll(from, CLIP, 500.0), window(3500.0, 3000.0));
    assert_eq!(Window::scroll(from, CLIP, -5000.0), window(0.0, 3000.0));
    assert_eq!(
        Window::scroll(from, CLIP, 99_999.0),
        window(12_360.0, 3000.0)
    );
}

#[test]
fn zooming_to_a_range_shows_exactly_the_range() {
    let range = BeatTime(3840)..BeatTime(7680);
    assert_eq!(Window::to_range(CLIP, &range), window(3840.0, 3840.0));
    // Narrower than the limit: centred in a window of the limit's width.
    let tight = Window::to_range(CLIP, &(BeatTime(1000)..BeatTime(1060))).unwrap();
    assert_eq!(tight.visible, MIN_VISIBLE);
    assert_eq!(tight.start, 1030.0 - MIN_VISIBLE / 2.0);
    // Near an end, the window stays inside the content.
    let end = Window::to_range(CLIP, &(BeatTime(15_300)..BeatTime(15_360))).unwrap();
    assert_eq!(
        (end.start, end.visible),
        (15_360.0 - MIN_VISIBLE, MIN_VISIBLE)
    );
    // The whole content is the fitted view, and an empty range has no window.
    assert_eq!(Window::to_range(CLIP, &(BeatTime(0)..CLIP)), None);
    assert_eq!(
        Window::to_range(CLIP, &(BeatTime(960)..BeatTime(960))),
        None
    );
}
