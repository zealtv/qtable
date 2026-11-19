//! Borrowing signal-path walk: nearest groups outward, then Main once.
use super::{Project, Track, TrackId};

#[derive(Clone, Copy, Debug)]
pub enum Ancestor<'a> {
    Group(&'a Track),
    Main(&'a Track),
}

impl<'a> Ancestor<'a> {
    pub fn group(self) -> Option<&'a Track> {
        match self {
            Self::Group(track) => Some(track),
            Self::Main(_) => None,
        }
    }

    pub fn delay_ms(self) -> f64 {
        match self {
            Self::Group(track) => track.delay_ms,
            Self::Main(main) => main.delay_ms,
        }
    }
}

pub struct Ancestors<'a> {
    project: &'a Project,
    subject: Option<TrackId>,
    next: Option<TrackId>,
    remaining: usize,
    main: bool,
}

impl Project {
    /// Excludes the subject; unknown IDs and Main have no ancestors.
    pub fn ancestors(&self, subject: TrackId) -> Ancestors<'_> {
        let track = self.track(subject).filter(|t| t.id != self.main.id);
        Ancestors {
            project: self,
            subject: track.map(|t| t.id),
            next: track.and_then(|t| t.group),
            remaining: self.tracks.len(),
            main: track.is_some(),
        }
    }
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = Ancestor<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining > 0
            && let Some(id) = self.next.take()
            && Some(id) != self.subject
            && let Some(group) = self.project.track(id).filter(|t| t.is_group())
        {
            self.remaining -= 1;
            self.next = group.group;
            return Some(Ancestor::Group(group));
        }
        self.next = None;
        if std::mem::take(&mut self.main) {
            Some(Ancestor::Main(&self.project.main))
        } else {
            None
        }
    }
}
