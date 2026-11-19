//! Track and project access to Target events.
use super::*;
use std::borrow::Cow;

impl Track {
    pub fn event_def(&self, reference: &EventRef) -> Option<(&Device, &EventDef)> {
        let device = self.device(reference.device)?;
        device
            .target()?
            .events
            .iter()
            .find(|event| event.key == reference.key)
            .map(|event| (device, event))
    }

    pub fn event_def_mut(&mut self, reference: &EventRef) -> Option<&mut EventDef> {
        let index = self.device_index(reference.device)?;
        self.devices[index]
            .target_mut()?
            .events
            .iter_mut()
            .find(|event| event.key == reference.key)
    }

    /// The event's label in lane labels, Info lines and the Target face: its display name,
    /// with `abs ` before an unnamed absolute path that another event also shows, then its
    /// signature (` (i f)`) when another event on the track still reads the same. Computed,
    /// never stored, so it follows every name and path edit. Empty for a ref that doesn't resolve.
    pub fn event_label<'a>(&'a self, reference: &EventRef) -> Cow<'a, str> {
        let Some((_, event)) = self.event_def(reference) else {
            return Cow::Borrowed("");
        };
        let events: Vec<&EventDef> = self.targets().flat_map(|(_, t)| &t.events).collect();
        let shows_absolute =
            |e: &EventDef| e.name.is_empty() && matches!(e.path, OscAddress::Absolute(_));
        let marked = |e: &'a EventDef| -> Cow<'a, str> {
            let shown = e.display_name();
            if shows_absolute(e)
                && events
                    .iter()
                    .any(|o| !shows_absolute(o) && o.display_name() == shown)
            {
                Cow::Owned(format!("abs {shown}"))
            } else {
                Cow::Borrowed(shown)
            }
        };
        let label = marked(event);
        if events
            .iter()
            .any(|o| !std::ptr::eq(*o, event) && marked(o) == label)
        {
            let signature: Vec<_> = event.signature().collect();
            Cow::Owned(format!("{label} ({})", signature_text(&signature)))
        } else {
            label
        }
    }

    /// How many lanes, declared or derived, play this event, and in how many clips.
    pub fn event_uses(&self, reference: &EventRef) -> (usize, usize) {
        self.slots
            .iter()
            .filter_map(|slot| slot.clip.as_ref())
            .map(|clip| clip.lane_refs().iter().filter(|r| *r == reference).count())
            .filter(|lanes| *lanes > 0)
            .fold((0, 0), |(lanes, clips), found| (lanes + found, clips + 1))
    }

    pub fn find_event(&self, path: &OscAddress) -> Option<EventRef> {
        self.targets().find_map(|(device, target)| {
            target
                .events
                .iter()
                .find(|event| &event.path == path)
                .map(|event| EventRef {
                    device: device.id,
                    key: event.key.clone(),
                })
        })
    }

    pub fn find_event_like(&self, definition: &EventDef) -> Option<EventRef> {
        self.targets().find_map(|(device, target)| {
            target
                .events
                .iter()
                .find(|event| {
                    event.path == definition.path && event.signature().eq(definition.signature())
                })
                .map(|event| EventRef {
                    device: device.id,
                    key: event.key.clone(),
                })
        })
    }

    pub fn first_target_mut_or_insert(&mut self, id: impl FnOnce() -> DeviceId) -> &mut Device {
        let index = match self.devices.iter().position(|d| d.target().is_some()) {
            Some(index) => index,
            None => {
                let index = self.insert_index(DeviceCategory::Target, None);
                self.devices.insert(index, NewDevice::Target.build(id()));
                index
            }
        };
        &mut self.devices[index]
    }

    pub fn ensure_event(
        &mut self,
        path: OscAddress,
        signature: &[FieldType],
        id: impl FnOnce() -> DeviceId,
    ) -> EventRef {
        let definition = EventDef {
            key: EntryKey(String::new()),
            name: String::new(),
            path,
            fields: signature
                .iter()
                .enumerate()
                .map(|(i, kind)| FieldDef::new(FieldKey(format!("f{}", i + 1)), *kind, false))
                .collect(),
        };
        if let Some(reference) = self.find_event_like(&definition) {
            return reference;
        }
        self.add_event(definition, id)
    }

    /// Copy a definition under a fresh key in the first Target, creating it when needed.
    /// Always adds an event, even when a matching path and signature already exist.
    pub fn add_event(&mut self, definition: EventDef, id: impl FnOnce() -> DeviceId) -> EventRef {
        let device = self.first_target_mut_or_insert(id);
        let target = device
            .target_mut()
            .expect("first_target_mut_or_insert returns a Target");
        let key = target.fresh_key(&definition.path);
        target.events.push(EventDef {
            key: key.clone(),
            ..definition
        });
        EventRef {
            device: device.id,
            key,
        }
    }
}

impl Project {
    /// Device-relative path; packet routing adds the destination and track paths.
    pub fn event_path(&self, track: TrackId, reference: &EventRef) -> Option<OscAddress> {
        let (device, definition) = self.track(track)?.event_def(reference)?;
        if !device.active {
            return None;
        }
        match &definition.path {
            OscAddress::Absolute(_) => Some(definition.path.clone()),
            OscAddress::Relative(_) => Some(OscAddress::Relative(resolve_address(
                device.target()?.path.as_str(),
                &definition.path,
            ))),
        }
    }
}

impl Track {
    /// A hit of the event at `path` whose signature is the arguments' types, creating that
    /// event (`ensure_event`) when the chain has none. Fixtures and tools build clips this way.
    pub fn hit(
        &mut self,
        position: BeatTime,
        path: OscAddress,
        arguments: Vec<OscArgument>,
        id: impl FnOnce() -> DeviceId,
    ) -> Event {
        let signature: Vec<_> = arguments.iter().map(FieldType::of).collect();
        Event {
            position,
            definition: self.ensure_event(path, &signature, id),
            arguments,
        }
    }
}

impl Project {
    /// `Track::hit` with a fresh project-wide device id; None for an unknown track.
    pub fn hit(
        &mut self,
        track: TrackId,
        position: BeatTime,
        path: OscAddress,
        arguments: Vec<OscArgument>,
    ) -> Option<Event> {
        let id = self.next_device_id();
        Some(self.track_mut(track)?.hit(position, path, arguments, || id))
    }

    /// `Track::ensure_event` with a fresh project-wide device id; None for an unknown track.
    pub fn ensure_event(
        &mut self,
        track: TrackId,
        path: OscAddress,
        signature: &[FieldType],
    ) -> Option<EventRef> {
        let id = self.next_device_id();
        Some(self.track_mut(track)?.ensure_event(path, signature, || id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_copies_metadata_under_fresh_keys_while_ensuring_reuses() {
        let mut track = Track::new(TrackId(1), "T", 0);
        let definition = EventDef {
            key: EntryKey("old-key".into()),
            name: "Cue".into(),
            path: OscAddress::Absolute("/cue".into()),
            fields: vec![FieldDef {
                name: "count".into(),
                ..FieldDef::new(FieldKey("f1".into()), FieldType::Int, false)
            }],
        };
        let first = track.add_event(definition.clone(), || DeviceId(7));
        let second = track.add_event(definition.clone(), || panic!("Target already exists"));
        assert_eq!(first.key.0, "cue");
        assert_eq!(second.key.0, "cue-2");
        assert_eq!(first.device, DeviceId(7));
        assert_eq!(second.device, first.device);
        assert_eq!(track.devices.len(), 1);
        for reference in [&first, &second] {
            let (_, copied) = track.event_def(reference).unwrap();
            assert_eq!(
                *copied,
                EventDef {
                    key: reference.key.clone(),
                    ..definition.clone()
                }
            );
        }
        assert_eq!(
            track.ensure_event(definition.path, &[FieldType::Int], || panic!("must reuse")),
            first
        );
        track.event_def_mut(&second).unwrap().name = "Other cue".into();
        assert_eq!(track.event_def(&first).unwrap().1.name, "Cue");
        assert_eq!(track.event_def(&second).unwrap().1.name, "Other cue");
        assert!(
            track
                .event_def_mut(&EventRef {
                    device: DeviceId(8),
                    key: first.key.clone()
                })
                .is_none()
        );
        assert!(
            track
                .event_def_mut(&EventRef {
                    device: first.device,
                    key: EntryKey("missing".into())
                })
                .is_none()
        );
    }
}
