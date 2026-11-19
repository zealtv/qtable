use super::*;
use crate::control::LIVE_DARK;

#[test]
fn each_level_is_one_named_theme_ink() {
    assert_eq!(LIVE_DARK.rule_ink(Level::Group), LIVE_DARK.row_rule);
    assert_eq!(
        LIVE_DARK.rule_ink(Level::Deep),
        LIVE_DARK.control_contrast_frame
    );
    assert_eq!(
        LIVE_DARK.rule_ink(Level::Light),
        LIVE_DARK.surface_highlight
    );
}

#[test]
fn the_levels_run_from_darkest_to_lightest() {
    let grey = |level| LIVE_DARK.rule_ink(level).r;
    assert!(grey(Level::Deep) < grey(Level::Group));
    assert!(grey(Level::Group) < grey(Level::Light));
}

#[test]
fn a_rule_builds_in_either_axis_and_any_thickness() {
    let _: Element<'_, ()> = rule(&LIVE_DARK, Axis::Horizontal, Level::Group);
    let _: Element<'_, ()> = rule(&LIVE_DARK, Axis::Vertical, Level::Deep);
    let _: Element<'_, ()> = rule_sized(&LIVE_DARK, Axis::Horizontal, Level::Group, 2.0);
}
