//! Gesture state: edit only on release; clip/revision changes cancel stale gestures.
use super::*;
use iced::keyboard::{Key, key::Named};
impl Timeline {
    pub(super) fn interact(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if state.key != Some(self.key)
            || state.revision != self.context.revision
            || state.events_only != self.context.events_only
        {
            *state = State {
                key: Some(self.key),
                events_only: self.context.events_only,
                revision: self.context.revision,
                ..Default::default()
            };
        }
        let layout = self.layout(bounds);
        let axis = self.axis(&layout, crate::outputs::to_colour(self.clip.colour));
        if self.part == Part::Header {
            return self.interact_header(state, &layout, &axis, event, bounds, cursor);
        }
        let busy = state.drag.is_some()
            || state.point.is_some()
            || state.paint.is_some()
            || state.range.is_some()
            || state.region.is_some()
            || state.header_drag.is_some();
        if let Some(action) = ruler::wheel(
            &axis,
            (self.key, self.context.window),
            ruler::WheelKeys {
                command: self.context.command_down,
                shift: self.context.shift_down,
            },
            busy,
            event,
            bounds,
            cursor,
        ) {
            return Some(action);
        }
        let grid = if self.context.command_down {
            Grid::Off
        } else {
            self.grid
        };
        if matches!(event, canvas::Event::Mouse(mouse::Event::ButtonPressed(_))) {
            state.range = None;
            state.region = None;
        }
        // Background selection owns its press through release, before point or hit gestures.
        if state.range.is_some() {
            match event {
                canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
                | canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    let global = cursor.position().or({
                        if let canvas::Event::Mouse(mouse::Event::CursorMoved { position }) = event
                        {
                            Some(*position)
                        } else {
                            None
                        }
                    });
                    let drag = state.range.as_mut()?;
                    let at = global
                        .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y))
                        .unwrap_or(drag.pointer);
                    let lo = at.y.min(drag.origin.y);
                    let hi = at.y.max(drag.origin.y);
                    let lanes = self
                        .model
                        .rows
                        .iter()
                        .filter(|r| !matches!(&r.kind, RowKind::Envelope(e) if e.region.is_some()))
                        .filter(|r| {
                            RULER_HEIGHT + r.top <= hi && RULER_HEIGHT + r.top + r.height >= lo
                        })
                        .map(|r| match &r.kind {
                            RowKind::Event { reference, .. } => {
                                lanes::Identity::Event(reference.clone())
                            }
                            RowKind::Envelope(e) => lanes::Identity::Envelope(e.target.clone()),
                        })
                        .collect();
                    let selection = drag.update(at, self.range_time(&layout, at.x), lanes);
                    if matches!(event, canvas::Event::Mouse(mouse::Event::ButtonReleased(_))) {
                        state.range = None;
                    }
                    return Some(
                        canvas::Action::publish(Message::Timeline(Action::Content(selection)))
                            .and_capture(),
                    );
                }
                _ => {}
            }
        }
        if let Some(region) = &mut state.region {
            region.follow(event, bounds, cursor);
            if matches!(
                event,
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            ) {
                let region = state.region.take()?;
                if region.moved {
                    state.point = None;
                    state.drag = None;
                    return Some(
                        canvas::Action::publish(Message::Timeline(Action::RegionMove(region)))
                            .and_capture(),
                    );
                }
            } else if matches!(
                event,
                canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
            ) && region.moved
            {
                return Some(canvas::Action::request_redraw().and_capture());
            }
        }
        let publish = |action| canvas::Action::publish(Message::Timeline(action)).and_capture();
        let target = |index| HitTarget {
            key: self.key,
            index,
            revision: self.context.revision,
        };
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(bounds) =>
            {
                let point = cursor.position_in(bounds)?;
                state.drag = None;
                state.header_drag = None;
                state.last_click = None;
                if let Some(row) = layout.header_at(point, &self.model) {
                    return Some(publish(Action::LaneMenu(
                        super::super::event_editor::lanes::Target {
                            key: self.key,
                            revision: self.context.revision,
                            row: self.source_row(row),
                        },
                    )));
                }
                self.right_press(state, point, &layout)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let point = cursor.position_in(bounds)?;
                if self.context.draw
                    && let Some(action) = self.press_paint(state, point, &layout, grid)
                {
                    return Some(action);
                }
                if let Some(index) = layout.hit(point, &self.clip.events, &self.model) {
                    let initial = self.clip.events[index].position;
                    let identity =
                        lanes::Identity::Event(self.clip.events[index].definition.clone());
                    if self
                        .context
                        .content
                        .as_ref()
                        .is_some_and(|s| s.contains(&identity, initial) || s.hits.contains(&index))
                        && !self.context.shift_down
                    {
                        state.region = Some(content::Move {
                            key: self.key,
                            revision: self.context.revision,
                            from: point,
                            to: point,
                            copy: self.context.alt_down,
                            moved: false,
                            time_scale: layout.axis.ticks_per_px(),
                            value_scale: 0.0,
                        });
                        return Some(canvas::Action::request_redraw().and_capture());
                    }
                    if self.context.shift_down {
                        return Some(publish(Action::SelectAt(target(index))));
                    }
                    state.drag = Some(Drag {
                        index,
                        origin_x: point.x,
                        initial,
                        preview: initial,
                        moved: false,
                        target: target(index),
                        copy: self.context.alt_down,
                    });
                    state.last_click = None;
                    return Some(publish(Action::SelectAt(target(index))));
                }
                if let Some(row) = layout.header_at(point, &self.model) {
                    state.last_click = None;
                    if let RowKind::Envelope(envelope) = &self.model.rows[row].kind
                        && headers::dot_hit(self, row, point)
                        && self.live[row].is_some_and(|l| l.overridden())
                    {
                        return Some(publish(Action::ReEnable(envelope.field.clone())));
                    }
                    // Generated event rows keep device definition order in both tabs.
                    if matches!(self.model.rows[row].kind, RowKind::Event { .. }) {
                        return Some(publish(Action::Header(self.key, self.source_row(row))));
                    }
                    let selected = self
                        .context
                        .lane_range
                        .filter(|(a, b)| (*a..*b).contains(&row));
                    let (start, end) = selected.unwrap_or((row, row + 1));
                    if self.model.rows[start..end]
                        .iter()
                        .any(|r| matches!(r.kind, RowKind::Event { .. }))
                    {
                        return Some(publish(Action::Header(self.key, row)));
                    }
                    state.header_drag = Some(HeaderDrag {
                        target: super::super::event_editor::lanes::Target {
                            key: self.key,
                            revision: self.context.revision,
                            row,
                        },
                        from: start..end,
                        origin: point,
                        cursor: point,
                        boundary: row,
                    });
                    // Keep a selected range intact when grabbing one of its headers.
                    return Some(if selected.is_some() && !self.context.shift_down {
                        canvas::Action::request_redraw().and_capture()
                    } else {
                        publish(Action::Header(self.key, row))
                    });
                }

                if point.x < LABEL_WIDTH {
                    state.last_click = None;
                    return None;
                }
                // The ruler header is its own canvas; this one's top band lies under it.
                if point.y < RULER_HEIGHT {
                    state.last_click = None;
                    return None;
                }
                let position = layout.position_at(point.x, grid);
                let Some(row) = layout.lane_at(point, &self.model) else {
                    state.last_click = None;
                    return Some(publish(Action::Cursor(self.key, position, None)));
                };
                let RowKind::Event {
                    index: _,
                    reference,
                } = &self.model.rows[row].kind
                else {
                    return self.press_point(state, row, point, &layout);
                };
                let now = crate::gesture_clock::now();
                let address = reference.clone();
                let double = state
                    .last_click
                    .as_ref()
                    .is_some_and(|(time, previous, lane)| {
                        now.duration_since(*time) <= Duration::from_millis(500)
                            && point.distance(*previous) <= 5.0
                            && *lane == address
                    });
                if double {
                    state.last_click = None;
                    Some(publish(Action::Add(self.key, position, address)))
                } else {
                    state.last_click = Some((now, point, address));
                    Some(self.begin_range(state, point, row, &layout))
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if state.paint.is_some() =>
            {
                let at = Point::new(position.x - bounds.x, position.y - bounds.y);
                self.follow_paint(state, at, &layout);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.paint.is_some() =>
            {
                if let Some(p) = cursor.position() {
                    self.follow_paint(state, Point::new(p.x - bounds.x, p.y - bounds.y), &layout);
                }
                let paint = state.paint.take()?;
                Some(match self.paint_action(paint) {
                    Some(action) => publish(action),
                    None => canvas::Action::request_redraw().and_capture(),
                })
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
                if state.header_drag.is_some() =>
            {
                let point = cursor.land().position()?;
                let drag = state.header_drag.as_mut()?;
                if let Some((a, b)) = self.context.lane_range {
                    drag.from = a..b;
                }
                drag.cursor = Point::new(
                    (point.x - bounds.x).clamp(0.0, bounds.width),
                    (point.y - bounds.y).clamp(0.0, bounds.height),
                );
                drag.boundary = self
                    .model
                    .rows
                    .iter()
                    .enumerate()
                    .find(|(_, r)| drag.cursor.y < RULER_HEIGHT + r.top + r.height)
                    .map_or(self.model.rows.len(), |(i, r)| {
                        crate::selection::header_boundary(
                            i,
                            drag.cursor.y,
                            RULER_HEIGHT + r.top,
                            r.height,
                        )
                    });
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.header_drag.is_some() =>
            {
                let drag = state.header_drag.take()?;
                if drag.dragging() {
                    let to = drag.destination();
                    Some(publish(Action::MoveLanes(drag.target, drag.from, to)))
                } else {
                    Some(publish(Action::Header(self.key, drag.target.row)))
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if state.point.is_some() =>
            {
                let local = Point::new(position.x - bounds.x, position.y - bounds.y);
                let keys = self.keys();
                let drag = state.point.as_mut()?;
                let lane = self.lane(drag.row, &layout)?;
                drag.gesture.drag(&lane, local, keys);
                let text = drag
                    .gesture
                    .readout(&lane)
                    .and_then(|r| self.readout(drag.row, r));
                if drag.gesture.previewing() {
                    state.last_alt = None;
                }
                Some(self.hover_action(state, text))
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.drag.is_none() => {
                state.pointer = cursor.position_in(bounds);
                let text = cursor.position_in(bounds).and_then(|point| {
                    let row = self.model.row_at(point.y - RULER_HEIGHT)?;
                    if let Some(region) = self.envelope(row).and_then(|e| e.region) {
                        return Some(if region.looping {
                            format!(
                                "Unlinked · {} loop · edit in One lane",
                                super::super::musical_time::format_duration(
                                    region.length(),
                                    self.signature
                                )
                            )
                        } else {
                            "Unlinked · one shot · edit in One lane".into()
                        });
                    }
                    let value = self.lane(row, &layout)?.hover(point)?;
                    self.envelope(row).map(|e| e.value_text(value))
                });
                Some(self.hover_action(state, text))
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.point.is_some() =>
            {
                let drag = state.point.take()?;
                let at = cursor.position().map_or(drag.gesture.pointer(), |p| {
                    Point::new(p.x - bounds.x, p.y - bounds.y)
                });
                let marker = drag.gesture.marker_click(at);
                let commit = self
                    .lane(drag.row, &layout)
                    .and_then(|lane| drag.gesture.release(&lane, at, self.keys()));
                Some(match commit {
                    Some(commit) => {
                        state.hover = None;
                        publish(Action::SetPoints(drag.lane, commit.points, commit.select))
                    }
                    None if marker => {
                        let action = self.begin_range(state, at, drag.row, &layout);
                        state.range = None;
                        action
                    }
                    // A release that changed nothing still ends the readout and the preview.
                    None => self.hover_action(state, None).and_capture(),
                })
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if state.drag.is_some() =>
            {
                let drag = state.drag.as_mut()?;
                Self::preview_drag(drag, &layout, position.x - bounds.x, grid);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.drag.take()?;
                if let Some(point) = cursor.position() {
                    Self::preview_drag(&mut drag, &layout, point.x - bounds.x, grid);
                }
                if drag.moved && drag.copy {
                    Some(publish(Action::Duplicate(drag.target, drag.preview)))
                } else if drag.moved && drag.preview != drag.initial {
                    Some(publish(Action::Move(drag.target, drag.preview)))
                } else {
                    Some(canvas::Action::request_redraw().and_capture())
                }
            }
            canvas::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) if state.drag.is_some()
                || state.paint.is_some()
                || state.point.is_some()
                || state.header_drag.is_some()
                || state.range.is_some()
                || state.region.is_some() =>
            {
                state.range = None;
                state.region = None;
                state.drag = None;
                state.point = None;
                state.paint = None;
                state.header_drag = None;
                state.last_click = None;
                Some(publish(Action::ClearContent))
            }
            canvas::Event::Window(iced::window::Event::Unfocused)
            | canvas::Event::Mouse(mouse::Event::CursorLeft) => {
                state.range = None;
                state.region = None;
                state.drag = None;
                state.point = None;
                state.paint = None;
                state.header_drag = None;
                state.pointer = None;
                state.last_click = None;
                Some(self.hover_action(state, None))
            }
            _ => None,
        }
    }
    /// The pinned header: the brace and flags, the zoom drag and wheels, and the insert strip.
    /// Nothing here depends on how far the lanes below have scrolled.
    fn interact_header(
        &self,
        state: &mut State,
        layout: &Layout,
        axis: &ruler::Axis,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if let Some(action) = ruler::interact(
            &mut state.marker,
            axis,
            (self.key, self.context.revision, None),
            event,
            bounds,
            cursor,
            self.context.command_down,
        ) {
            return Some(action);
        }
        if let Some(action) = ruler::zoom(
            &mut state.zoom,
            axis,
            (self.key, self.context.window),
            event,
            bounds,
            cursor,
        ) {
            return Some(action);
        }
        let busy = state.marker.is_some() || state.zoom.dragging();
        if let Some(action) = ruler::wheel(
            axis,
            (self.key, self.context.window),
            ruler::WheelKeys {
                command: self.context.command_down,
                shift: self.context.shift_down,
            },
            busy,
            event,
            bounds,
            cursor,
        ) {
            return Some(action);
        }
        if matches!(
            event,
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && cursor.is_over(bounds)
            && let Some(point) = cursor.position_in(bounds)
            && point.x >= LABEL_WIDTH
            && (super::super::event_timeline_math::INSERT_STRIP_TOP..RULER_HEIGHT)
                .contains(&point.y)
        {
            let at = self.range_time(layout, point.x);
            let lanes = self
                .context
                .content
                .as_ref()
                .map(|s| s.lanes.clone())
                .unwrap_or_else(|| {
                    self.model
                        .rows
                        .first()
                        .map(|r| {
                            vec![match &r.kind {
                                RowKind::Event { reference, .. } => {
                                    lanes::Identity::Event(reference.clone())
                                }
                                RowKind::Envelope(e) => lanes::Identity::Envelope(e.target.clone()),
                            }]
                        })
                        .unwrap_or_default()
                });
            return Some(
                canvas::Action::publish(Message::Timeline(Action::Content(
                    content::Selection::time(self.key, at..at, lanes),
                )))
                .and_capture(),
            );
        }
        None
    }

    /// A right-click on content: a hit, an envelope point, or lane space. Anywhere else (the
    /// ruler, the label column, an unlinked row) it opens nothing.
    fn right_press(
        &self,
        state: &mut State,
        point: Point,
        layout: &Layout,
    ) -> Option<canvas::Action<Message>> {
        state.paint = None;
        state.last_alt = None;
        if point.x < LABEL_WIDTH || point.y < RULER_HEIGHT {
            return None;
        }
        let click = |lane, under| {
            canvas::Action::publish(Message::Timeline(Action::ContentMenu(menu::Click {
                key: self.key,
                revision: self.context.revision,
                lane,
                under,
            })))
            .and_capture()
        };
        if let Some(index) = layout.hit(point, &self.clip.events, &self.model) {
            let lane = lanes::Identity::Event(self.clip.events[index].definition.clone());
            return Some(click(lane, menu::Under::Hit(index)));
        }
        let row = layout.lane_at(point, &self.model)?;
        let space = menu::Under::Space(self.range_time(layout, point.x));
        match &self.model.rows[row].kind {
            RowKind::Event { reference, .. } => {
                Some(click(lanes::Identity::Event(reference.clone()), space))
            }
            // An unlinked lane has its own timeline: only its One lane view edits it.
            RowKind::Envelope(envelope) if envelope.region.is_some() => None,
            RowKind::Envelope(envelope) => {
                let lane = lanes::Identity::Envelope(envelope.target.clone());
                let under = match self.lane(row, layout)?.hit(point) {
                    super::super::point_gesture::Hit::Point(index) => menu::Under::Point(index),
                    _ => space,
                };
                Some(click(lane, under))
            }
        }
    }
    /// Publishes the readout when it changed; otherwise only asks for the redraw a preview needs.
    fn hover_action(&self, state: &mut State, text: Option<String>) -> canvas::Action<Message> {
        if state.hover == text {
            return canvas::Action::request_redraw();
        }
        state.hover = text.clone();
        canvas::Action::publish(Message::Timeline(Action::Hover(text)))
    }
    /// A press in an envelope row's content starts a point gesture, or with Alt held twice
    /// straightens the segment there. Nothing publishes until the release decides.
    fn press_point(
        &self,
        state: &mut State,
        row: usize,
        at: Point,
        layout: &Layout,
    ) -> Option<canvas::Action<Message>> {
        state.last_click = None;
        if self.envelope(row)?.region.is_some() {
            return Some(canvas::Action::request_redraw().and_capture());
        }
        let lane = self.lane(row, layout)?;
        let target = self.point_target(row)?;
        let keys = self.keys();
        let identity = lanes::Identity::Envelope(target.target.clone());
        let hit = lane.hit(at);
        if keys.shift
            && let super::super::point_gesture::Hit::Point(index) = hit
        {
            return Some(
                canvas::Action::publish(Message::Timeline(Action::PickPoint(target, index)))
                    .and_capture(),
            );
        }
        if hit == super::super::point_gesture::Hit::Empty {
            return Some(self.begin_range(state, at, row, layout));
        }
        let region = self.context.content.as_ref().is_some_and(|s| s.contains(&identity, lane.time_at(at.x, keys)) || matches!(hit, super::super::point_gesture::Hit::Point(i) if s.points.contains(&(target.target.clone(),i))));
        if region {
            state.region = Some(content::Move {
                key: self.key,
                revision: self.context.revision,
                from: at,
                to: at,
                copy: keys.alt,
                moved: false,
                time_scale: layout.axis.ticks_per_px(),
                value_scale: 1.0 / f64::from(lane.plot.height),
            });
        }
        if keys.alt && !region {
            let now = crate::gesture_clock::now();
            let double = state
                .last_alt
                .take()
                .is_some_and(|(time, previous, earlier)| {
                    earlier == row
                        && now.duration_since(time) <= Duration::from_millis(500)
                        && previous.distance(at) <= 5.0
                });
            if double {
                return Some(match lane.straighten(at) {
                    Some(commit) => canvas::Action::publish(Message::Timeline(Action::SetPoints(
                        target,
                        commit.points,
                        None,
                    )))
                    .and_capture(),
                    None => canvas::Action::request_redraw().and_capture(),
                });
            }
            state.last_alt = Some((now, at, row));
        } else {
            state.last_alt = None;
        }
        let selected = self
            .point
            .filter(|_| self.envelope.as_ref() == Some(&target.target));
        state.point = Gesture::press(&lane, at, keys, selected).map(|gesture| PointDrag {
            gesture,
            row,
            lane: target,
        });
        Some(canvas::Action::request_redraw().and_capture())
    }
    /// A press in Draw Mode on a lane's content starts a stroke instead of a selection or a
    /// point gesture. `None` leaves headers, the ruler and empty space to the usual handling.
    fn press_paint(
        &self,
        state: &mut State,
        at: Point,
        layout: &Layout,
        grid: Grid,
    ) -> Option<canvas::Action<Message>> {
        let over = layout.hit(at, &self.clip.events, &self.model);
        let row = match over {
            Some(index) => self.model.event_row(&self.clip.events[index].definition)?,
            None => layout.lane_at(at, &self.model)?,
        };
        state.last_click = None;
        state.last_alt = None;
        let keys = self.keys();
        let painting = match &self.model.rows[row].kind {
            RowKind::Envelope(envelope) => {
                // An unlinked lane is read-only in this view (design section 2).
                if envelope.region.is_some() {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                let lane = self.lane(row, layout)?;
                super::super::draw::Painting::envelope(&lane, envelope.threshold, keys, at)
            }
            RowKind::Event { reference, .. } => {
                let stroke = super::super::draw::HitStroke::new(
                    grid.ticks(),
                    layout.length(),
                    self.clip
                        .events
                        .iter()
                        .filter(|e| &e.definition == reference)
                        .map(|e| e.position),
                );
                super::super::draw::Painting::hits(stroke, layout.ticks_at(at.x), over, at)
            }
        };
        state.paint = Some(Paint { row, painting });
        Some(canvas::Action::request_redraw().and_capture())
    }
    fn follow_paint(&self, state: &mut State, at: Point, layout: &Layout) {
        let keys = self.keys();
        let Some(paint) = &mut state.paint else {
            return;
        };
        let lane = self.lane(paint.row, layout);
        paint
            .painting
            .push(lane.as_ref(), at, layout.ticks_at(at.x), keys);
    }
    /// The stroke as the one action its release publishes.
    fn paint_action(&self, paint: Paint) -> Option<Action> {
        use super::super::draw::Outcome;
        let Paint { row, painting } = paint;
        Some(Action::Draw(match painting.finish()? {
            Outcome::Points(replacement) => Draw::Points {
                lane: self.point_target(row)?,
                replacement,
            },
            Outcome::Hits { span, positions } => {
                let RowKind::Event { reference, .. } = &self.model.rows[row].kind else {
                    return None;
                };
                Draw::Hits {
                    key: self.key,
                    revision: self.context.revision,
                    lane: reference.clone(),
                    span,
                    positions,
                }
            }
            Outcome::Erase(index) => Draw::Erase(HitTarget {
                key: self.key,
                index,
                revision: self.context.revision,
            }),
        }))
    }
    pub(super) fn range_time(&self, layout: &Layout, x: f32) -> BeatTime {
        let ticks = layout.axis.unclamped_ticks_at(x).max(0.0);
        let step = if self.context.command_down {
            1
        } else {
            self.grid.ticks()
        };
        BeatTime(
            ((ticks / step as f64).round() as u64)
                .saturating_mul(step)
                .min(layout.length().ticks()),
        )
    }
    fn begin_range(
        &self,
        state: &mut State,
        at: Point,
        row: usize,
        layout: &Layout,
    ) -> canvas::Action<Message> {
        let identity = match &self.model.rows[row].kind {
            RowKind::Event { reference, .. } => lanes::Identity::Event(reference.clone()),
            RowKind::Envelope(e) => lanes::Identity::Envelope(e.target.clone()),
        };
        let drag = content::Drag::new(self.key, at, self.range_time(layout, at.x), identity);
        let selection = drag.selection.clone();
        state.range = Some(drag);
        canvas::Action::publish(Message::Timeline(Action::Content(selection))).and_capture()
    }
    fn preview_drag(drag: &mut Drag, layout: &Layout, x: f32, grid: Grid) {
        let delta = x - drag.origin_x;
        drag.moved |= delta.abs() >= 3.0;
        if drag.moved {
            drag.preview = layout.dragged(drag.initial, delta, grid);
        }
    }
}
