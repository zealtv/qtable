//! Engine ownership and the state held by the GUI.
use crate::{drafts::Drafts, fields::format_tempo, files::title_text, grid, selection, status};
use qtable_engine::engine::{Engine, Snapshot};
use qtable_engine::model::Project;
use std::path::PathBuf;

pub(crate) struct App {
    pub(crate) command_context: Option<qtable_engine::engine::CommandContext>,
    /// Which edits join one undo step; GUI session state.
    pub(crate) gesture: crate::gesture_steps::State,
    pub(crate) engine: Engine,
    pub(crate) remote: crate::remote::State,
    pub(crate) snapshot: Snapshot,
    pub(crate) tempo_input: String,
    pub(crate) status: status::StatusLine,
    pub(crate) path: Option<PathBuf>,
    /// The engine's content identity at the last save, load or start; the project is
    /// unsaved whenever its content differs. An undo back to this state shows saved.
    pub(crate) saved_content: u64,
    /// Held through the prompt and any ensuing save, preventing repeated close dialogs.
    pub(crate) file_intent_pending: bool,
    pub(crate) drafts: Drafts,
    /// Settings of a destination whose host was cleared (Main), kept for the session so retyping
    /// a host restores them. Not part of the set, so a save does not carry them.
    pub(crate) dormant_destinations:
        std::collections::HashMap<qtable_engine::model::TrackId, qtable_engine::model::Output>,
    pub(crate) clip_view: crate::inspector::clip_panels::State,
    pub(crate) detail_layout: crate::detail_layout::State,
    /// The control with the white click outline; GUI session state, not part of the set.
    pub(crate) control_focus: crate::drafts::ControlFocus,
    /// Colours every control reads; swapping it reskins them all.
    pub(crate) theme: crate::control::Theme,
    pub(crate) pending_field: Option<crate::drafts::DraftKey>,
    /// Whether the last message other than `FieldEntered` reported an error or left a field's
    /// rejected value pending: what Enter's release asks.
    pub(crate) last_message_rejected: bool,
    pub(crate) selection: selection::SelectionState,
    pub(crate) clip_actions: crate::clip_actions::ClipActions,
    pub(crate) browser: crate::browser::State,
    /// MIDI Map Mode: the switch, the control waiting to learn and the selected list row.
    pub(crate) map_mode: crate::map_mode::State,
    pub(crate) device_drag: Option<crate::browser::Drag>,
    pub(crate) device_chain: crate::device_chain::State,
    pub(crate) header_actions: crate::header_actions::HeaderActions,
    /// The window region that has keyboard focus; GUI session state, not part of the set.
    pub(crate) focus: crate::focus::Focus,
    pub(crate) tick_count: u64,
    pub(crate) project_generation: u64,
    pub(crate) grid_scroll_x: u32,
    pub(crate) output_scroll_x: u32,
    /// On Release drags held by field, for the Mixer and the Target face alike.
    pub(crate) parameter_gesture: crate::parameter_gesture::State,
    /// Brief visual acknowledgement of manual event commands (pointer, MIDI and keys).
    pub(crate) scene_stop_feedback: crate::scene_stop_feedback::State,
    pub(crate) event_feedback: crate::event_feedback::State,
    pub(crate) grid_layout: grid::LayoutCache,
    /// Region header gestures shared by the pinned headers and scrolling slots.
    pub(crate) header_gestures: grid::header::Gestures,
    /// Keeps the selected scene in view as the grid's scene rows shrink.
    pub(crate) scene_follow: grid::scene_follow::SceneFollow,
    /// Window regions live for this GUI session, independently of project files.
    pub(crate) chrome: crate::chrome::State,
    /// Browser width and Detail View height, set by dragging the dividers.
    pub(crate) dividers: crate::dividers::Sizes,
    pub(crate) info: crate::info::State,
    /// The one open popover; GUI session state, not part of the set.
    pub(crate) overlay: Option<crate::overlay::Overlay>,
    /// The row the keyboard has reached in the open menu or chooser list (`menu_keys.rs`):
    /// an index among its pressable rows. Reset whenever the overlay changes.
    pub(crate) menu_highlight: Option<usize>,
    pub(crate) cursor: iced::Point,
    pub(crate) fast_poll_until: Option<std::time::Instant>,
}

impl App {
    pub(crate) fn new(engine: Engine, tempo_input: String, path: Option<PathBuf>) -> Self {
        let snapshot = engine.snapshot();
        let saved_content = engine.content_id();
        let status = status::StatusLine::for_project(engine.project());
        let gesture = crate::gesture_steps::State::new(engine.gesture());
        Self {
            command_context: None,
            gesture,
            engine,
            remote: Default::default(),
            snapshot,
            tempo_input,
            status,
            path,
            saved_content,
            file_intent_pending: false,
            drafts: Drafts::default(),
            dormant_destinations: Default::default(),
            clip_view: Default::default(),
            detail_layout: Default::default(),
            control_focus: Default::default(),
            theme: crate::control::LIVE_DARK,
            pending_field: None,
            last_message_rejected: false,
            selection: selection::SelectionState::default(),
            clip_actions: Default::default(),
            browser: Default::default(),
            map_mode: Default::default(),
            device_drag: None,
            device_chain: Default::default(),
            header_actions: Default::default(),
            focus: crate::focus::Focus::default(),
            tick_count: 0,
            project_generation: 0,
            grid_scroll_x: 0,
            output_scroll_x: 0,
            parameter_gesture: Default::default(),
            scene_stop_feedback: Default::default(),
            event_feedback: Default::default(),
            grid_layout: grid::LayoutCache::default(),
            header_gestures: Default::default(),
            scene_follow: grid::scene_follow::SceneFollow::default(),
            chrome: crate::chrome::State::default(),
            dividers: crate::dividers::Sizes::default(),
            info: crate::info::State::default(),
            overlay: None,
            menu_highlight: None,
            cursor: iced::Point::ORIGIN,
            fast_poll_until: None,
        }
    }

    /// The update router commits pending fields before changing section visibility.
    pub(crate) fn toggle_detail_section(
        &mut self,
        tab: crate::inspector_panel::Tab,
        section: crate::detail_layout::SectionId,
    ) {
        self.detail_layout.toggle(tab, section);
        self.control_focus.clear();
        self.close_overlay();
        self.focus = crate::focus::Focus::Detail;
    }

    pub(crate) fn project(&self) -> &Project {
        self.engine.project()
    }

    pub(crate) fn title(&self) -> String {
        title_text(self.path.as_deref(), self.is_dirty())
    }

    /// True when the project's content differs from what was last saved or loaded. Compares
    /// the engine's content identity, which an undo or redo restores with the content.
    pub(crate) fn is_dirty(&self) -> bool {
        self.engine.content_id() != self.saved_content
    }

    /// Records the current content as the saved state.
    pub(crate) fn mark_saved(&mut self) {
        self.saved_content = self.engine.content_id();
    }

    /// Control constructors for this window's theme and focus.
    pub(crate) fn controls(&self) -> crate::drafts::Controls<'_> {
        crate::drafts::Controls::new(self.theme, &self.control_focus)
            .shift(self.selection.shift_down)
    }

    /// The engine's count of project mutations; caches and save stamps key on it.
    pub(crate) fn project_revision(&self) -> u64 {
        self.engine.project_revision()
    }

    /// Refresh the model and discard UI state belonging to the previous project.
    /// Call only after the engine has successfully loaded the replacement.
    pub(crate) fn reset_for_project(&mut self) {
        self.remote.ingress.discard_pending();
        self.command_context = None;
        crate::gesture_steps::reset(self);
        self.remote.router.reset();
        self.project_generation = self.project_generation.wrapping_add(1);
        self.dormant_destinations.clear();
        self.fast_poll_until = None;
        self.scene_stop_feedback = Default::default();
        self.event_feedback = Default::default();
        self.control_focus.clear();
        self.close_overlay();
        self.header_actions = Default::default();
        self.header_gestures = Default::default();
        self.device_chain = Default::default();
        self.device_drag = None;
        self.browser = Default::default();
        self.map_mode.reset_for_project();
        self.info.update(crate::info::Hover::Clear);
        self.grid_scroll_x = 0;
        self.output_scroll_x = 0;
        self.snapshot = self.engine.snapshot();
        self.tempo_input = format_tempo(self.project().tempo.bpm());
        self.status = status::StatusLine::for_project(self.project());
        self.mark_saved();
        self.drafts = Drafts::default();
        self.pending_field = None;
        let editor_epoch = self.clip_view.editor.epoch.wrapping_add(1);
        let lane_clipboard = self.clip_view.editor.lane_clipboard.take();
        let (alt_down, shift_down, command_down, draw) = {
            let editor = &self.clip_view.editor;
            (
                editor.alt_down,
                editor.shift_down,
                editor.command_down,
                editor.draw,
            )
        };
        self.clip_view.editor = Default::default();
        self.clip_view.editor.lane_clipboard = lane_clipboard;
        self.clip_view.editor.alt_down = alt_down;
        self.clip_view.editor.shift_down = shift_down;
        self.clip_view.editor.command_down = command_down;
        self.clip_view.editor.draw = draw;
        self.clip_view.editor.epoch = editor_epoch;
        if matches!(
            self.clip_actions.clipboard,
            Some(crate::clip_actions::GridClipboard::Tracks { .. })
        ) {
            self.clip_actions.clipboard = None;
        }
        self.clip_actions.renaming = None;
        let shift_down = self.selection.shift_down;
        self.selection = selection::SelectionState {
            shift_down,
            ..Default::default()
        };
    }
}
