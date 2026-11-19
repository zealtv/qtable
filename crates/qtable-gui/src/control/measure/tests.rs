use super::*;

const FACE_SIZE: f32 = 11.0;

fn face() -> Face {
    Face::canvas(FACE_SIZE)
}

#[test]
fn a_label_that_fits_is_returned_whole() {
    let whole = width("Opacity", face());
    assert_eq!(fit("Opacity", whole, face()), "Opacity");
    assert_eq!(fit_ellipsis("Opacity", whole, face()), "Opacity");
}

#[test]
fn fit_keeps_the_longest_prefix_that_measures_within_the_room() {
    let room = width("Opaci", face());
    assert_eq!(fit("Opacity", room, face()), "Opaci");
    assert!(width(&fit("Opacity", room - 0.5, face()), face()) <= room - 0.5);
}

#[test]
fn fit_ellipsis_includes_the_ellipsis_in_the_room() {
    let room = width("Opaci…", face());
    let shown = fit_ellipsis("Opacity", room, face());
    assert_eq!(shown, "Opaci…");
    assert!(width(&shown, face()) <= room);
}

#[test]
fn a_very_narrow_room_still_shows_the_first_character() {
    assert_eq!(fit("Opacity", 0.0, face()), "O");
    assert_eq!(fit_ellipsis("Opacity", 0.0, face()), "O…");
}

#[test]
fn clipping_follows_width_not_character_count() {
    let narrow = "i".repeat(30);
    let wide = "W".repeat(30);
    let room = width(&"W".repeat(10), face());
    assert_eq!(
        fit(&narrow, room, face()),
        narrow,
        "thirty narrow letters fit"
    );
    assert!(fit(&wide, room, face()).chars().count() <= 10);
}

#[test]
fn multibyte_text_is_cut_on_a_character_boundary() {
    let label = "ééééééééé";
    let room = width("éééé", face());
    assert_eq!(fit(label, room, face()), "éééé");
}

#[test]
fn the_value_font_measures_wider_than_nothing() {
    let face = Face::value(Pixels(FACE_SIZE));
    assert!(width("/bop/drums", face) > width("/", face));
    assert_eq!(width("", face), 0.0);
}
