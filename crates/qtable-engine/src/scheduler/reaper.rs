//! Frees retired projects and automation histories off the realtime thread.
use std::io;
use std::sync::mpsc::{self, Sender};
use std::thread;

/// Owns a small thread that drops whatever it is sent. An edit replaces the
/// planner's `Arc<Project>`; if the scheduler held the last reference, freeing
/// the whole tree would happen between two sends.
pub(crate) struct Reaper {
    retired: Sender<Box<dyn Send>>,
}

impl Reaper {
    pub(crate) fn spawn() -> io::Result<Self> {
        let (retired, queue) = mpsc::channel::<Box<dyn Send>>();
        thread::Builder::new()
            .name("qtable-reaper".into())
            .spawn(move || queue.iter().for_each(drop))?;
        Ok(Self { retired })
    }

    /// Hand over a reference. If the thread has gone, it drops here instead.
    pub(crate) fn retire(&self, value: impl Send + 'static) {
        let _ = self.retired.send(Box::new(value));
    }
}
