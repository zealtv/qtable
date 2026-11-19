//! Pure OSC composition shared by playback and previews.
use super::{Ancestor, DeviceId, OscAddress, Project, Track, TrackId};

/// Trim boundary slashes only; preserve interior slashes and root suffixes.
/// Absolute addresses bypass the path verbatim.
pub fn resolve_address(path: &str, address: &OscAddress) -> String {
    match address {
        OscAddress::Absolute(address) => address.clone(),
        OscAddress::Relative(address) => format!(
            "{}/{}",
            path.trim_end_matches('/'),
            address.trim_start_matches('/')
        ),
    }
}

/// Which piece a field edits: the track Path, a piece under it, or a device entry path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathLevel {
    Track,
    Under,
    Device(DeviceId),
}

impl Project {
    /// Where a track's paths resolve: itself when it is a destination, else its routed,
    /// configured destinations (active or not), Main first.
    pub fn path_destinations(&self, source: TrackId) -> Vec<&Track> {
        let Some(track) = self.track(source) else {
            return Vec::new();
        };
        if track.capabilities().destination {
            return Some(track)
                .filter(|t| t.destination().is_some())
                .into_iter()
                .collect();
        }
        let route = self.resolved_route(source);
        self.configured_destinations()
            .filter(|d| route.contains(&d.id))
            .collect()
    }

    /// The inherited part before the piece at `level`, for one destination: destination
    /// path, outer-to-inner group paths, then (Under and Device) the source's own path. Device also adds its own path. The
    /// source's own path is skipped when it sends to itself, and at Track level a
    /// destination's own Path inherits nothing.
    pub fn inherited_path(
        &self,
        source: TrackId,
        destination: TrackId,
        level: PathLevel,
    ) -> Option<String> {
        let own = self.track(source)?;
        self.destination(destination)?.destination()?;
        if level == PathLevel::Track && source == destination {
            return Some(String::new());
        }
        let mut groups: Vec<&Track> = self.ancestors(source).filter_map(Ancestor::group).collect();
        groups.reverse();
        let under = (level != PathLevel::Track && source != destination).then_some(&own.path);
        let namespace = [self.destination(destination).map(|t| &t.path)]
            .into_iter()
            .flatten()
            .chain(groups.into_iter().map(|g| &g.path))
            .chain(under)
            .fold(String::new(), |all, path| all + path.as_str());
        match level {
            PathLevel::Device(id) => Some(namespace + own.device(id)?.target()?.path.as_str()),
            _ => Some(namespace),
        }
    }

    /// `inherited_path` for each of `path_destinations`.
    pub fn inherited_paths(&self, source: TrackId, level: PathLevel) -> Vec<(&Track, String)> {
        self.path_destinations(source)
            .into_iter()
            .filter_map(|d| self.inherited_path(source, d.id, level).map(|p| (d, p)))
            .collect()
    }

    /// Destination path, outer-to-inner group paths, the source's own path (unless it sends
    /// to itself), then the address. Absolute addresses bypass every path. Main's path
    /// enters only as a destination, never as an ancestor.
    pub fn resolve_event_address(
        &self,
        source: TrackId,
        destination: TrackId,
        address: &OscAddress,
    ) -> Option<String> {
        let namespace = self.inherited_path(source, destination, PathLevel::Under)?;
        if let OscAddress::Absolute(address) = address {
            return Some(address.clone());
        }
        Some(resolve_address(&namespace, address))
    }
}
