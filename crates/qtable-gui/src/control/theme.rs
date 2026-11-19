//! Named colour tokens every control reads for every state. Swapping the `Theme` value
//! reskins all controls; clip and track colours stay project data.
use iced::Color;

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgb(
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    )
}

/// Field names follow the `.ask` tokens of Live 12's Default Dark Neutral Medium
/// where one exists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Theme {
    pub surface_background: Color,
    pub detail_view_background: Color,
    pub surface_highlight: Color,
    /// qtable title strips: one clear step above the device body.
    pub device_title_background: Color,
    pub control_background: Color,
    pub display_background: Color,
    /// TransportOffBackground / TransportSelectionBackground from the retained .ask.
    pub transport_off_background: Color,
    pub transport_selection_background: Color,
    /// ControlSelectionFrame; the numeric Chrome adapter uses this lighter role on press.
    pub control_selection_frame: Color,
    /// The observed white phase of a numeric caret; Live's .ask has no caret-white key.
    pub numeric_cursor: Color,
    /// ChosenRecord; the red phase of numeric typing, not an invalid-value colour.
    pub chosen_record: Color,
    pub control_contrast_frame: Color,
    /// ControlForeground and ControlOffForeground share one value in the retained theme.
    pub control_foreground: Color,
    pub text_disabled: Color,
    /// Mid ink for a path's immediate parent.
    pub path_parent: Color,
    /// ChosenDefault / ViewControlOn: an enabled toggle.
    pub chosen_default: Color,
    pub control_on_foreground: Color,
    /// Control On (Inactive), `live_control_zombie`: a lit toggle in an inactive device.
    pub control_on_inactive: Color,
    /// Control On Variant, `live_value_arc`: a Dial's value arc.
    pub value_arc: Color,
    /// Control Range On (Inactive), `live_value_arc_zombie`: the arc or bar of a control that is
    /// inactive or doesn't apply.
    pub value_arc_inactive: Color,
    /// Slider Range Value, `live_value_bar`: a Slider's value bar.
    pub value_bar: Color,
    pub selection_background: Color,
    pub chosen_play: Color,
    pub chosen_alert: Color,
    pub alert: Color,
    /// SearchIndication: the retained theme's yellow hint, used for status notices.
    pub search_indication: Color,
    /// A lit Solo switch.
    pub chosen_alternative: Color,
    /// OrangeVuMeter: MIDI-style OSC activity.
    pub orange_vu_meter: Color,
    /// LearnMidi: the blue Map Mode lays over controls a MIDI controller can drive.
    pub learn_midi: Color,
    /// Live .ask LearnKey.
    pub learn_key: Color,
    /// The focus stroke's ink: Live's slider corner brackets, grey 183 in the 2026-10-16
    /// capture.
    pub focus_stroke: Color,
    /// Corner radii in logical px, measured on Live 12: a device
    /// frame, a cell or panel inside a device, and a control (field, button, toggle, readout).
    pub corner_device: f32,
    pub corner_cell: f32,
    pub corner_control: f32,
    /// Length of one arm of the focus brackets drawn on a Slider or Dial.
    pub focus_bracket_arm: f32,
    pub fader_well: Color,
    pub fader_edge: Color,
    pub fader_handle: Color,
    /// Fader centre line, tick marks and level mark (qtable choices, sampled from Live's fader).
    pub fader_centre: Color,
    pub fader_tick_major: Color,
    pub fader_tick_minor: Color,
    pub fader_mark: Color,
    /// SurfaceArea: a track's body behind its slots, and the region surface.
    pub surface_area: Color,
    /// Desktop (also ClipBorder, SceneContrast): the Session grid's gaps.
    pub desktop: Color,
    /// qtable's own: an empty slot's box, one step above `surface_area`.
    pub slot_background: Color,
    /// qtable's own: the launch area behind a playing or follow-queued clip's icon.
    pub launch_active: Color,
    /// qtable's own: white washes over the selected row and column, strength a qtable choice.
    pub row_tint: Color,
    pub column_tint: Color,
    /// qtable's own: lift under a hovered (low alpha) or pressed (high alpha) launch button.
    pub press_wash: Color,
    pub press_wash_pressed: Color,
    /// qtable's own: the Session status row's cell fill and border.
    pub status_row_background: Color,
    pub status_row_border: Color,
    /// qtable's own: a Stop button whose track has a stop pending.
    pub stop_pending: Color,
    /// qtable's own: the drag strip between panels.
    pub divider: Color,
    /// qtable's own: the event timeline's selected lane, lighter than both row shades.
    pub lane_selected: Color,
    /// AutomationColor: an envelope that follows its clip, and its lane's dot.
    pub automation_color: Color,
    /// AutomationMouseOver: the hovered segment of an envelope and its two points. Live 12 draws
    /// it near-white in the curve's hue (its teal line (16, 139, 153) hovers as (148, 253, 252),
    /// sampled from a recording, 2026-10-27), so this lightens AutomationColor the same way.
    pub automation_mouse_over: Color,
    /// qtable's own: a loop brace's inward end triangles (Live draws them grey on an outlined
    /// brace; qtable fills its brace with the role ink, so these are dark).
    pub brace_end_mark: Color,
    /// AutomationDisabled: an envelope whose argument a manual change has overridden.
    pub automation_disabled: Color,
    /// qtable's own: a selection whose region lacks focus.
    pub selection_inactive: Color,
    /// ClipText: ink on a light clip colour.
    pub clip_text: Color,
    /// qtable's own: ink on a dark clip colour or a lit chooser.
    pub clip_text_light: Color,
    /// qtable's own: the device chain behind the device bodies (sampled from Live, 2026-10-15).
    pub chain_background: Color,
    /// qtable's own: the rule that opens each strip row, darker than the panel.
    pub row_rule: Color,
    /// qtable's own: a group or row with no single owning colour (rgb 179 as written, not
    /// merged into `control_foreground` 181).
    pub neutral: Color,
}

pub(crate) const LIVE_DARK: Theme = Theme {
    fader_well: rgb(15, 15, 15),
    fader_edge: rgb(34, 34, 34),
    fader_handle: rgb(216, 216, 216),
    fader_centre: rgb(64, 64, 64),
    fader_tick_major: rgb(134, 134, 134),
    fader_tick_minor: rgb(93, 93, 93),
    fader_mark: rgb(170, 170, 170),
    surface_area: rgb(36, 36, 36),
    desktop: Color::from_rgb(42.0 / 255.0, 42.0 / 255.0, 42.0 / 255.0),
    slot_background: rgb(48, 48, 48),
    launch_active: Color::BLACK,
    row_tint: Color {
        a: 0.10,
        ..Color::WHITE
    },
    column_tint: Color {
        a: 0.08,
        ..Color::WHITE
    },
    press_wash: Color {
        a: 0.14,
        ..Color::WHITE
    },
    press_wash_pressed: Color {
        a: 0.32,
        ..Color::WHITE
    },
    status_row_background: Color::from_rgb(0.20, 0.20, 0.20),
    status_row_border: Color::from_rgb(0.36, 0.36, 0.36),
    stop_pending: Color::from_rgb(0.65, 0.22, 0.18),
    divider: rgb(16, 16, 16),
    automation_color: rgb(255, 77, 71),
    automation_mouse_over: rgb(255, 210, 206),
    automation_disabled: rgb(100, 100, 100),
    brace_end_mark: rgb(40, 40, 40),
    lane_selected: Color::from_rgb(78.0 / 255.0, 78.0 / 255.0, 78.0 / 255.0),
    selection_inactive: rgb(104, 125, 133),
    clip_text: Color::BLACK,
    clip_text_light: Color::WHITE,
    chain_background: rgb(43, 43, 43),
    row_rule: Color::from_rgb(0.145, 0.145, 0.145),
    neutral: Color::from_rgb(0.7, 0.7, 0.7),
    surface_background: rgb(54, 54, 54),
    detail_view_background: rgb(62, 62, 62),
    surface_highlight: rgb(70, 70, 70),
    device_title_background: rgb(86, 86, 86),
    control_background: rgb(30, 30, 30),
    display_background: rgb(24, 24, 24),
    transport_off_background: rgb(54, 54, 54),
    transport_selection_background: rgb(93, 93, 93),
    control_selection_frame: rgb(145, 145, 145),
    numeric_cursor: Color::WHITE,
    chosen_record: rgb(255, 85, 89),
    control_contrast_frame: rgb(17, 17, 17),
    control_foreground: rgb(181, 181, 181),
    text_disabled: rgb(117, 117, 117),
    path_parent: rgb(149, 149, 149),
    chosen_default: rgb(255, 173, 86),
    control_on_foreground: rgb(7, 7, 7),
    control_on_inactive: rgb(117, 117, 117),
    value_arc: rgb(3, 195, 213),
    value_arc_inactive: rgb(117, 117, 117),
    value_bar: rgb(0, 115, 131),
    selection_background: rgb(176, 221, 235),
    chosen_play: rgb(0, 211, 141),
    chosen_alert: rgb(231, 105, 66),
    alert: rgb(231, 105, 66),
    search_indication: rgb(236, 202, 109),
    chosen_alternative: rgb(3, 195, 213),
    orange_vu_meter: rgb(255, 165, 25),
    learn_midi: rgb(64, 52, 239),
    learn_key: rgb(255, 100, 0),
    focus_stroke: rgb(183, 183, 183),
    corner_device: 3.0,
    corner_cell: 2.0,
    corner_control: 1.0,
    focus_bracket_arm: 4.0,
};

/// Dark clip text, the ink on light backgrounds.
pub(crate) const DARK_INK: Color = LIVE_DARK.clip_text;

fn luminance(c: Color) -> f32 {
    fn linear(value: f32) -> f32 {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// Keeps coloured headers and clips readable, including old black colours.
pub(crate) fn ink(background: Color) -> Color {
    let light = luminance(background) + 0.05;
    if light / (luminance(DARK_INK) + 0.05) >= 1.05 / light {
        DARK_INK
    } else {
        LIVE_DARK.clip_text_light
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compares four chrome colours with Live 12's `Default Dark Neutral Medium.ask`. The file
    /// is Ableton's and isn't distributed with qtable, so the test reads it only where
    /// `QTABLE_LIVE_THEME_ASK` names a copy. With the variable unset the test skips and says
    /// so on stderr; with it set, a missing or empty file fails the test, so a wrong path
    /// can't pass silently.
    #[test]
    fn numeric_chrome_roles_match_the_retained_live_theme() {
        let Some(path) = std::env::var_os("QTABLE_LIVE_THEME_ASK") else {
            eprintln!("skipped: QTABLE_LIVE_THEME_ASK names no Live theme file");
            return;
        };
        let path = std::path::PathBuf::from(path);
        let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "QTABLE_LIVE_THEME_ASK is set to {}, which can't be read: {error}",
                path.display()
            )
        });
        assert!(
            !source.trim().is_empty(),
            "QTABLE_LIVE_THEME_ASK is set to {}, which is empty",
            path.display()
        );
        for (name, color) in [
            ("TransportOffBackground", LIVE_DARK.transport_off_background),
            (
                "TransportSelectionBackground",
                LIVE_DARK.transport_selection_background,
            ),
            ("ControlSelectionFrame", LIVE_DARK.control_selection_frame),
            ("ChosenRecord", LIVE_DARK.chosen_record),
        ] {
            let [r, g, b, _] = color.into_rgba8();
            let entry = format!("<{name} Value=\"#{r:02x}{g:02x}{b:02x}\"");
            assert!(
                source.contains(&entry),
                "{name} must use its retained Live theme value"
            );
        }
    }

    /// The look literals moved into the theme on 2026-10-16 keep the
    /// values they had where they were written.
    #[test]
    fn moved_literals_keep_their_values() {
        let t = LIVE_DARK;
        assert_eq!(t.surface_area, Color::from_rgb8(36, 36, 36));
        assert_eq!(t.desktop, Color::from_rgb8(42, 42, 42));
        assert_eq!(t.slot_background, Color::from_rgb8(48, 48, 48));
        assert_eq!(t.divider, Color::from_rgb8(16, 16, 16));
        assert_eq!(t.automation_color, Color::from_rgb8(255, 77, 71));
        assert_eq!(t.automation_mouse_over, Color::from_rgb8(255, 210, 206));
        assert_eq!(t.automation_disabled, Color::from_rgb8(100, 100, 100));
        assert_eq!(t.selection_inactive, Color::from_rgb8(104, 125, 133));
        assert_eq!(t.chain_background, Color::from_rgb8(43, 43, 43));
        assert_eq!(t.row_rule, Color::from_rgb(0.145, 0.145, 0.145));
        assert_eq!(t.neutral, Color::from_rgb(0.7, 0.7, 0.7));
        assert_eq!(t.fader_centre, Color::from_rgb8(64, 64, 64));
        assert_eq!(t.fader_tick_major, Color::from_rgb8(134, 134, 134));
        assert_eq!(t.fader_tick_minor, Color::from_rgb8(93, 93, 93));
        assert_eq!(t.fader_mark, Color::from_rgb8(170, 170, 170));
        assert_eq!(
            (t.clip_text, t.clip_text_light),
            (Color::BLACK, Color::WHITE)
        );
        assert_eq!(t.launch_active, Color::BLACK);
        assert_eq!((t.row_tint.a, t.column_tint.a), (0.10, 0.08));
        assert_eq!((t.press_wash.a, t.press_wash_pressed.a), (0.14, 0.32));
        assert_eq!(t.status_row_background, Color::from_rgb(0.20, 0.20, 0.20));
        assert_eq!(t.status_row_border, Color::from_rgb(0.36, 0.36, 0.36));
        assert_eq!(t.stop_pending, Color::from_rgb(0.65, 0.22, 0.18));
    }

    #[test]
    fn kept_apart_values_stay_apart() {
        assert_ne!(LIVE_DARK.chain_background, LIVE_DARK.desktop);
        assert_ne!(LIVE_DARK.neutral, LIVE_DARK.control_foreground);
    }

    #[test]
    fn all_palette_swatches_and_legacy_black_have_readable_text() {
        use qtable_engine::model::LIVE_CLIP_PALETTE;
        for background in LIVE_CLIP_PALETTE
            .into_iter()
            .map(|c| Color::from_rgb8(c.red, c.green, c.blue))
            .chain([Color::BLACK])
        {
            let a = luminance(background);
            let b = luminance(ink(background));
            let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(contrast >= 4.5, "contrast {contrast} on {background:?}");
        }
        assert_eq!(ink(Color::BLACK), Color::WHITE);
        assert_eq!(ink(Color::WHITE), DARK_INK);
    }
}
