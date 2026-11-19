use super::*;
use qtable_engine::model::{Project, Region, TrackId};
fn area(project: &Project) -> OutputArea<'_> {
    let cols = super::super::layout::columns(project, Region::Outputs);
    OutputArea {
        scenes: &project.scenes,
        selected_row: None,
        outputs: &project.outputs,
        selected_tracks: &[],
        owner: None,
        cell: None,
        layout: Rc::new(Layout {
            header_h: crate::grid::HEADER_H,
            xs: super::super::layout::x_positions(&cols),
            columns: cols,
        }),
        visible: true,
        revision: 0,
        drag: Default::default(),
        drag_viewport: Default::default(),
        drag_h: 400.,
        shift_down: false,
    }
}
fn event(
    area: &OutputArea<'_>,
    state: &mut HeaderState,
    event: canvas::Event,
    at: Point,
) -> Option<Action> {
    area.update(
        state,
        &event,
        Rectangle::new(Point::ORIGIN, Size::new(500.0, 400.0)),
        mouse::Cursor::Available(at),
    )
    .and_then(|action| action.into_inner().0)
}
#[test]
fn output_canvas_press_drag_release_reorders_and_click_only_selects() {
    use mouse::{Button, Event::*};
    let project = Project {
        outputs: (201..=203)
            .map(|id| Track::new_output(TrackId(id), "Out", "localhost", 9000))
            .collect(),
        ..Project::default()
    };
    let area = area(&project);
    let mut state = HeaderState::default();
    let start = Point::new(20.0, 10.0);
    assert_eq!(
        event(
            &area,
            &mut state,
            canvas::Event::Mouse(ButtonPressed(Button::Left)),
            start
        ),
        Some(Action::PressHeader(Box::new(Action::SelectColumn(
            crate::selection::ColumnOwner::Track(TrackId(201)),
            None
        ))))
    );
    assert_eq!(
        event(
            &area,
            &mut state,
            canvas::Event::Mouse(ButtonReleased(Button::Left)),
            start
        ),
        None
    );
    let _ = event(
        &area,
        &mut state,
        canvas::Event::Mouse(ButtonPressed(Button::Left)),
        start,
    );
    let end = Point::new(300.0, 10.0);
    let _ = event(
        &area,
        &mut state,
        canvas::Event::Mouse(CursorMoved { position: end }),
        end,
    );
    let drag = area.drag.get().unwrap();
    assert_eq!(
        header::drop_at(
            area.outputs,
            &area.layout.columns,
            &area.layout.xs,
            drag.key,
            drag.cursor
        )
        .unwrap()
        .marker,
        header::Marker::Before(3)
    );
    assert_eq!(
        event(
            &area,
            &mut state,
            canvas::Event::Mouse(ButtonReleased(Button::Left)),
            end
        ),
        Some(Action::PlaceTrack {
            track: TrackId(201),
            before: None,
            group: None
        })
    );
    assert!(area.drag.get().is_none());
}
#[test]
fn canvas_menus_name_headers_and_empty_region_space() {
    let project = Project {
        outputs: vec![Track::new_output(TrackId(201), "Out", "localhost", 9000)],
        ..Project::default()
    };
    let area = area(&project);
    let mut state = HeaderState::default();
    let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    assert_eq!(
        event(&area, &mut state, right.clone(), Point::new(20.0, 10.0)),
        Some(Action::HeaderMenu(super::super::TrackId(201)))
    );
    assert_eq!(
        event(&area, &mut state, right, Point::new(400.0, 10.0)),
        Some(Action::RegionMenu(Region::Outputs))
    );
}
