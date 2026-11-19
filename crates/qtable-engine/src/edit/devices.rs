//! Atomic device-chain edits; selection and clipboards belong to clients.
use super::{EditReport, lanes};
use crate::model::{Device, DeviceId, ModelError, Project, Track, TrackId};

fn err(message: &str) -> ModelError {
    ModelError(message.into())
}

fn check_device(track: &Track, device: &Device) -> Result<(), ModelError> {
    if !track
        .capabilities()
        .accepts_device(device.kind.capabilities().category)
    {
        return Err(err(
            "Targets require a Track with clips; Groups, Outputs and Main cannot host them",
        ));
    }
    Ok(())
}

fn check_before(track: &Track, before: Option<DeviceId>) -> Result<(), ModelError> {
    if before.is_some_and(|id| track.device(id).is_none()) {
        return Err(err("insertion device must be on the same track"));
    }
    Ok(())
}

/// Resolve the whole selection first. Repeated ids select a device just once.
fn owner(project: &Project, devices: &[DeviceId]) -> Result<TrackId, ModelError> {
    let first = devices.first().ok_or_else(|| err("no devices selected"))?;
    let (track, _) = project
        .device(*first)
        .ok_or_else(|| err("unknown device"))?;
    for id in devices {
        let (other, _) = project.device(*id).ok_or_else(|| err("unknown device"))?;
        if other.id != track.id {
            return Err(err("devices must be on one track"));
        }
    }
    Ok(track.id)
}

pub(super) fn paste(
    project: &mut Project,
    track: TrackId,
    devices: Vec<Device>,
    before: Option<DeviceId>,
) -> Result<(), ModelError> {
    if devices.is_empty() {
        return Err(err("nothing to paste"));
    }
    let next = project.next_device_id().0;
    let track = project
        .track_mut(track)
        .ok_or_else(|| err("unknown track"))?;
    check_before(track, before)?;
    for device in &devices {
        check_device(track, device)?;
    }
    let copies: Vec<_> = (next..)
        .zip(devices)
        .map(|(id, mut device)| {
            device.id = DeviceId(id);
            device
        })
        .collect();
    let mut before = before;
    // Work back from the insertion point, preserving order even when it is clamped.
    for device in copies.into_iter().rev() {
        let index = track.insert_index(device.kind.capabilities().category, before);
        before = Some(device.id);
        track.devices.insert(index, device);
    }
    Ok(())
}

pub(super) fn duplicate(project: &mut Project, devices: Vec<DeviceId>) -> Result<(), ModelError> {
    let track = owner(project, &devices)?;
    let source = project.track(track).expect("resolved above");
    let last = source
        .devices
        .iter()
        .rposition(|d| devices.contains(&d.id))
        .unwrap();
    let before = source.devices.get(last + 1).map(|d| d.id);
    let copies = source
        .devices
        .iter()
        .filter(|d| devices.contains(&d.id))
        .cloned()
        .collect();
    paste(project, track, copies, before)
}

pub(super) fn move_device(
    project: &mut Project,
    device: DeviceId,
    before: Option<DeviceId>,
) -> Result<(), ModelError> {
    let track = owner(project, &[device])?;
    let track = project.track_mut(track).expect("resolved above");
    check_before(track, before)?;
    check_device(track, track.device(device).expect("resolved above"))?;
    if before == Some(device) {
        return Ok(());
    }
    let index = track.device_index(device).expect("resolved above");
    let device = track.devices.remove(index);
    let index = track.insert_index(device.kind.capabilities().category, before);
    track.devices.insert(index, device);
    Ok(())
}

pub(super) fn remove(
    project: &mut Project,
    devices: Vec<DeviceId>,
) -> Result<EditReport, ModelError> {
    let track = owner(project, &devices)?;
    let track = project.track_mut(track).expect("resolved above");
    let report = lanes::remove_matching(track, |reference| devices.contains(&reference.device));
    track.devices.retain(|d| !devices.contains(&d.id));
    Ok(report)
}

pub(super) fn set_active(
    project: &mut Project,
    devices: Vec<DeviceId>,
    active: bool,
) -> Result<(), ModelError> {
    let track = owner(project, &devices)?;
    let track = project.track_mut(track).expect("resolved above");
    for device in track.devices.iter_mut().filter(|d| devices.contains(&d.id)) {
        device.active = active;
    }
    Ok(())
}
