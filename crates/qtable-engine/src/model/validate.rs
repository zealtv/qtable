//! Cross-reference, structure and OSC address validation.
use super::*;

impl Project {
    pub fn validate(&self) -> ModelResult {
        self.validate_with(validate_address, STREAM_RATE)
    }

    /// Validate the previous file invariants, retaining newly invalid paths and stream rates
    /// so clients can display and repair them one field at a time.
    pub(crate) fn validate_stored(&self) -> ModelResult {
        self.validate_with(validate_stored_address, parameter::STORED_STREAM_RATE)
    }

    fn validate_with(
        &self,
        address: fn(&str) -> ModelResult,
        stream_rate: ParameterSpec,
    ) -> ModelResult {
        Tempo::new(self.tempo.bpm())?;
        TimeSignature::new(
            self.time_signature.numerator(),
            self.time_signature.denominator(),
        )?;
        if let GlobalQuantisation::Value(value) = self.global_quantisation {
            value.validate()?;
        }
        self.validate_colours()?;
        self.validate_strip_ids()?;
        let output_ids = unique_ids(self.destinations().map(|o| o.id), "output")?;
        unique_ids(self.tracks.iter().map(|t| t.id), "track")?;
        let scene_ids = unique_ids(self.scenes.iter().map(|s| s.id), "scene")?;
        unique_ids(
            self.tracks
                .iter()
                .flat_map(|t| &t.slots)
                .filter_map(|s| s.clip.as_ref().map(|c| c.id)),
            "clip",
        )?;
        unique_ids(
            self.tracks
                .iter()
                .chain(&self.outputs)
                .chain([&self.main])
                .flat_map(|track| track.devices.iter().map(|device| device.id)),
            "device",
        )?;
        self.validate_output_tracks()?;
        self.validate_main()?;
        for track in self.tracks.iter().chain(&self.outputs).chain([&self.main]) {
            track.path.validate_with(address)?;
            track
                .mixer_device
                .validate_with(address, stream_rate.clone())?;
            track.validate_devices_with(address)?;
            if track.capabilities().destination && track.mixer_device.volume.output.enabled {
                require(
                    self.effective_delay_ms(track.id).abs() <= MAX_TRACK_DELAY_MS,
                    "combined mixer delay must be between -1000 and 1000 ms",
                )?;
            }
            for output in self.volume_recipients(track.id) {
                require(
                    self.volume_delay_ms(track.id, output.id).abs() <= MAX_TRACK_DELAY_MS,
                    "combined mixer delay must be between -1000 and 1000 ms",
                )?;
            }
        }
        self.validate_groups()?;
        for (name, delay) in self
            .group_tracks()
            .map(|g| ("group", g.delay_ms))
            .chain([("Main", self.main.delay_ms)])
        {
            require(
                delay.is_finite() && delay.abs() <= MAX_TRACK_DELAY_MS,
                &format!("{name} delay must be between -1000 and 1000 ms"),
            )?;
        }
        for track in &self.tracks {
            validate_route(&track.route, &output_ids)?;
            for output in self.routed_outputs(track.id) {
                require(
                    self.send_delay_ms(track.id, output.id).abs() <= MAX_TRACK_DELAY_MS,
                    "combined track, group and Main delay and output latency must be between -1000 and 1000 ms",
                )?;
            }
            require(track.delay_ms.is_finite(), "track delay must be finite")?;
            require(
                track.delay_ms.abs() <= MAX_TRACK_DELAY_MS,
                "track delay must be between -1000 and 1000 ms",
            )?;
            require(
                track.slots.len() == self.scenes.len(),
                "track must retain one slot per scene",
            )?;
            for clip in track.slots.iter().filter_map(|slot| slot.clip.as_ref()) {
                clip.validate()?;
                track.validate_clip(clip)?;
                FollowMultiplier::new(
                    clip.follow_actions.multiplier.numerator(),
                    clip.follow_actions.multiplier.denominator(),
                )?;
                validate_actions(&clip.follow_actions.actions, &scene_ids)?;
            }
        }
        for scene in &self.scenes {
            validate_actions(&scene.follow_actions.actions, &scene_ids)?;
        }
        self.validate_mappings()?;
        Ok(())
    }
}

fn unique_ids<T: Eq + std::hash::Hash>(
    ids: impl Iterator<Item = T>,
    kind: &str,
) -> ModelResult<HashSet<T>> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ModelError(format!("duplicate {kind} ID")));
        }
    }
    Ok(seen)
}

/// The group arrangement a project may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupShape {
    /// Format 10 and earlier: a group holds only clip tracks, directly after it.
    Flat,
    /// Format 11: groups may hold groups, in preorder.
    Forest,
}

/// Current files accept a preorder forest. Historical files retain the flat gate
/// at `file/v10.rs`, independently of current editing capabilities.
pub(crate) const CURRENT_GROUP_SHAPE: GroupShape = GroupShape::Forest;

impl Project {
    fn validate_groups(&self) -> ModelResult {
        validate_group_shape(&self.tracks, CURRENT_GROUP_SHAPE)
    }
}

/// The group rules of format 10 and earlier, which every older file passes where it enters the
/// current model. They stay flat however far the current format opens nesting.
pub(crate) fn validate_flat_groups(tracks: &[Track]) -> ModelResult {
    validate_group_shape(tracks, GroupShape::Flat)
}

/// Group tracks: each is followed directly by all its
/// members and holds no clips. Output references are not checked here; the route pass
/// in `validate_with` covers every track, so this reads only the track list.
pub(crate) fn validate_group_shape(tracks: &[Track], shape: GroupShape) -> ModelResult {
    let ids: HashSet<_> = tracks.iter().map(|t| t.id).collect();
    match shape {
        GroupShape::Flat => validate_flat_shape(tracks, &ids),
        GroupShape::Forest => validate_forest_shape(tracks),
    }
}

fn validate_flat_shape(tracks: &[Track], ids: &HashSet<TrackId>) -> ModelResult {
    for (index, track) in tracks.iter().enumerate() {
        if let Some(group) = track.group {
            require(group != track.id, "a track cannot be its own group")?;
            require(ids.contains(&group), "group references missing track")?;
            require(
                tracks.iter().any(|t| t.id == group && t.is_group()),
                "a track's group must be a group track",
            )?;
            require(
                !track.is_group(),
                "groups cannot be nested yet: a group track cannot have a group",
            )?;
        }
        if !track.is_group() {
            require(!track.fold_state, "only a group track can be folded")?;
            continue;
        }
        require_group_content(track)?;
        require(
            !matches!(track.route, Route::Group),
            "group route cannot inherit a group",
        )?;
        let members = tracks.iter().filter(|t| t.group == Some(track.id)).count();
        require(members > 0, "track group must not be empty")?;
        require(
            tracks[index + 1..]
                .iter()
                .take(members)
                .filter(|t| t.group == Some(track.id))
                .count()
                == members,
            "group tracks must be adjacent and in project order",
        )?;
    }
    Ok(())
}

/// Format 11's rules: the nesting is a depth-first forest (`Topology::build`), and a group
/// may inherit its parent's route but a top-level group has no parent to inherit from.
fn validate_forest_shape(tracks: &[Track]) -> ModelResult {
    let topology = Topology::build(tracks)?;
    require(
        topology.childless_groups().next().is_none(),
        "track group must not be empty",
    )?;
    for track in tracks {
        if !track.is_group() {
            require(!track.fold_state, "only a group track can be folded")?;
            continue;
        }
        require_group_content(track)?;
        require(
            track.group.is_some() || !matches!(track.route, Route::Group),
            "group route cannot inherit a group",
        )?;
    }
    Ok(())
}

fn require_group_content(group: &Track) -> ModelResult {
    require(
        group.slots.iter().all(|s| s.clip.is_none()),
        "a group track cannot hold clips",
    )
}

pub(crate) fn validate_route(route: &Route, ids: &HashSet<TrackId>) -> ModelResult {
    if let Route::Outputs(outputs) = route {
        unique_ids(outputs.iter().copied(), "route output")?;
        require(
            outputs.iter().all(|id| ids.contains(id)),
            "missing output reference",
        )?;
    }
    Ok(())
}

pub(super) fn validate_actions(actions: &FollowActions, scenes: &HashSet<SceneId>) -> ModelResult {
    for action in [actions.action_a, actions.action_b] {
        if let FollowAction::Jump(id) = action {
            require(
                scenes.contains(&id),
                "follow action jumps to missing scene row",
            )?;
        }
    }
    Ok(())
}

impl Project {
    /// Live never shows black, so black (`Colour::UNSET`) never survives into a valid project.
    fn validate_colours(&self) -> ModelResult {
        let clips = self
            .tracks
            .iter()
            .flat_map(|t| &t.slots)
            .filter_map(|s| s.clip.as_ref());
        let colours = self
            .outputs
            .iter()
            .map(|o| o.colour)
            .chain(self.tracks.iter().map(|t| t.colour))
            .chain(clips.map(|c| c.colour));
        for colour in colours {
            require(!colour.is_unset(), "colour must not be black")?;
        }
        Ok(())
    }
}
