//! Clip properties beside the clip editor: its Events | Envelopes | Overview tabs over the lanes,
//! and one shared footer for the selected hit or point below.
use super::Message;
use super::lane_model::RowKind;
use super::lane_view::EditorTab;
use crate::detail_layout::{self, Layout};
use crate::drafts::DraftKey;
use crate::info::{Help, Id};
use crate::style::{self, detail};
use iced::widget::{Space, column, container, row, scrollable, stack, text};
use iced::{Element, Length};
use qtable_engine::model::{Project, TrackId};
use std::collections::HashMap;

mod footer;

/// The widget id of the Events and Overview lane list, for capture hooks that scroll it.
pub(crate) const LANE_SCROLL: &str = "clip-editor-lane-list";

pub fn view(
    project: &Project,
    selected: Option<(TrackId, usize)>,
    drafts: &HashMap<DraftKey, String>,
    errors: &HashMap<DraftKey, String>,
    states: (&super::clip_panels::State, &detail_layout::State),
    controls: crate::drafts::Controls<'_>,
) -> Layout<'static, Message> {
    let (state, layout_state) = states;
    let Some((track, slot)) = selected else {
        return Layout::empty("Select a clip to inspect it.");
    };
    let Some(clip) = super::clip_panel_update::clip(project, track, slot) else {
        return Layout::empty("Selected slot is empty.");
    };
    let key = super::event_editor::ClipKey {
        track,
        slot,
        id: clip.id,
        epoch: state.editor.epoch,
    };
    let choice = state.editor.views.choice(project, track, clip.id);
    // One lane: the strip speaks of the chosen argument whether or not the clip has its lane.
    let selection = match choice.one_lane() {
        Some(target) => (
            None,
            None,
            Some(target.clone()),
            state.editor.point_for(key),
        ),
        None => (
            state.editor.lane_for(key, clip),
            state.editor.selected_for(key),
            state.editor.envelope_for(key),
            state.editor.point_for(key),
        ),
    };
    let fields = super::event_strip::Fields {
        track,
        slot,
        drafts,
        errors,
        controls,
    };
    // Grid keeps Live's far right; Envelopes' choosers and markers live below the lane.
    let tools = row![Space::new().width(Length::Fill)]
        .push(text("Grid").size(detail::TEXT_SIZE))
        .push(
            detail::dropdown(
                super::event_timeline_math::Grid::ALL,
                Some(state.editor.grid),
                |grid| Message::Timeline(super::event_editor::Action::Grid(grid)),
            )
            .width(76)
            .info(Id::EventGrid, Message::Info),
        )
        .spacing(detail::CONTROL_GAP)
        .align_y(iced::alignment::Vertical::Center)
        .height(detail::HEADER_HEIGHT);
    let (cached_clip, model) =
        state
            .editor
            .model_cache
            .get(project, key, state.editor.revision, clip);
    let model = if choice.tab == EditorTab::Events {
        state.editor.model_cache.events()
    } else {
        model
    };
    let live = model.live(project, &state.editor.automation);
    let context = super::event_timeline::Context {
        events_only: choice.tab == EditorTab::Events,
        content: state.editor.content.clone(),
        lane_range: super::event_editor::lanes::range(
            &state.editor,
            key,
            project.track(track),
            clip,
        ),
        revision: state.editor.revision,
        alt_down: state.editor.alt_down,
        shift_down: state.editor.shift_down,
        command_down: state.editor.command_down,
        draw: state.editor.draw.on,
        focused: state.editor.focused,
        // One window for the clip, shared by all three tabs.
        window: state.editor.zooms.window(
            track,
            clip.id,
            super::clip_markers::extent(clip, project.time_signature),
        ),
    };
    // Envelopes' argument as the lane draws it, and its value now (also the choosers' LEDs).
    let chosen = choice.one_lane().map(|target| {
        let row = super::lane_view::chosen_row(project, track, &model, target);
        let live = super::lane_model::live_value(project, &state.editor.automation, &row.field);
        (target, row, live)
    });
    let live_chosen = chosen.as_ref().and_then(|(_, _, live)| *live);
    let body: Element<'static, Message> = match (choice.tab, chosen) {
        // All editor: the lane spans the tab, with only its value scale at the left, as Live's.
        (EditorTab::Envelopes, Some((target, chosen, live_chosen))) => {
            let laned = model
                .rows
                .iter()
                .any(|r| matches!(&r.kind, RowKind::Envelope(e) if e.target == *target));
            let unlinked = model.rows.iter().any(
                |r| matches!(&r.kind, RowKind::Envelope(e) if e.target == *target && e.region.is_some()),
            );
            super::lane_view::view(super::lane_view::Input {
                clip: cached_clip,
                model: model.clone(),
                live: &live,
                chosen,
                live_chosen,
                laned,
                key,
                point: selection.3,
                grid: state.editor.grid,
                signature: project.time_signature,
                playhead: state.editor.playhead_for(target, unlinked),
                context,
                hover: state.editor.hover.clone(),
            })
        }
        (EditorTab::Envelopes, None) => container(
            text("This device has no argument to automate. Choose another in the Device chooser.")
                .size(detail::TEXT_SIZE)
                .color(style::DISABLED),
        )
        .padding(detail::PANEL_INSET)
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        (EditorTab::Events | EditorTab::Overview, _) => {
            let empty = model.rows.is_empty().then(|| empty_hint(choice.tab));
            let parts = super::event_timeline::view(
                (cached_clip, model, live),
                key,
                selection.clone(),
                (
                    state.editor.grid,
                    project.time_signature,
                    state.editor.playhead,
                    state.editor.lane_playheads.clone(),
                ),
                (context, state.editor.hover.clone()),
            );
            let lanes = scrollable(parts.lanes)
                .id(iced::widget::Id::new(LANE_SCROLL))
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(detail::scrollbar()));
            // The ruler header is the top layer, over the scrolled lanes: it stays where it is
            // while they scroll under it, as in Live's clip editor, and it takes the pointer over
            // its band, so the lanes beneath never see a press meant for the ruler.
            let header = container(parts.header)
                .width(Length::Fill)
                .height(Length::Shrink);
            // Over the canvas, under the ruler and right of the header strip, so the ruler and brace
            // stay usable on an empty clip.
            match empty {
                Some(hint) => stack![
                    lanes,
                    container(text(hint).size(detail::TEXT_SIZE).color(style::DISABLED)).padding(
                        iced::Padding {
                            top: super::event_timeline_math::RULER_HEIGHT
                                + f32::from(detail::PANEL_INSET),
                            left: super::event_timeline_math::LABEL_WIDTH
                                + f32::from(detail::PANEL_INSET),
                            ..iced::Padding::ZERO
                        }
                    ),
                    header
                ]
                .into(),
                None => stack![lanes, header].into(),
            }
        }
    };
    let toolbar = row![
        super::lane_view::tab_bar(&controls, (track, slot), &choice),
        tools.width(Length::Fill)
    ]
    .spacing(detail::CONTROL_GAP);
    let editor = column![
        toolbar,
        body,
        footer::view(
            project,
            key,
            state.editor.revision,
            (&choice, live_chosen),
            &fields,
            state
                .editor
                .content
                .as_ref()
                .and_then(|s| s.summary(project.time_signature))
                .map_or_else(
                    || super::event_strip::view(project, clip, &fields, selection),
                    |summary| detail::panel(text(summary).size(detail::TEXT_SIZE)).into()
                ),
        ),
    ]
    .spacing(detail::CONTROL_GAP)
    .width(Length::Fill)
    .height(Length::Fill);
    let mut layout = super::clip_panels::sections(
        project,
        clip,
        (track, slot),
        drafts,
        errors,
        (state, layout_state),
        controls,
    );
    layout.content = Some(editor.into());
    layout
}

/// What an empty Events or Overview says: why it is empty and how to fill it, in one sentence.
/// Rows come from the track's Target devices (`lanes-come-from-the-device-not-the-clip`), so the
/// way in is the device, or for Overview also an envelope drawn on the Envelopes tab.
pub(super) fn empty_hint(tab: EditorTab) -> &'static str {
    match tab {
        EditorTab::Events => "No events yet. Add one to a Target device on this track.",
        _ => {
            "No events or envelopes yet. Add events to a Target device, or draw an envelope on the Envelopes tab."
        }
    }
}
