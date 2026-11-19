//! One enqueue stamp for every scheduler producer, including endpoint resolution.
use super::*;

#[derive(Debug)]
pub(crate) struct Envelope {
    pub(super) command: SchedulerCommand,
    pub(super) enqueued_at: Instant,
    pub(super) submission: Option<crate::engine::Submission>,
    // Frozen at first dequeue, before this command's own hold.
    pub(super) admitted: Option<super::commands::admission::Decision>,
}

#[derive(Clone)]
pub(crate) struct CommandSender(Sender<Envelope>);

impl CommandSender {
    pub(crate) fn channel() -> (Self, Receiver<Envelope>) {
        let (tx, rx) = mpsc::channel();
        (Self(tx), rx)
    }

    pub(crate) fn send(&self, command: SchedulerCommand) -> io::Result<()> {
        self.send_with_context(command, None)
    }

    pub(crate) fn send_with_context(
        &self,
        command: SchedulerCommand,
        submission: Option<crate::engine::Submission>,
    ) -> io::Result<()> {
        // Stamp at the shared seam, not at Engine::execute or at scheduler dequeue.
        // Concurrent producers may stamp in a different order from channel insertion:
        // received order, never timestamp order, is authoritative.
        self.0
            .send(Envelope {
                command,
                enqueued_at: Instant::now(),
                submission,
                admitted: None,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "scheduler stopped"))
    }
}

#[cfg(test)]
impl SchedulerCommand {
    pub(super) fn enqueued(self, enqueued_at: Instant) -> Envelope {
        Envelope {
            command: self,
            enqueued_at,
            submission: None,
            admitted: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_senders_stamp_at_enqueue_and_retain_channel_order() {
        let (sender, rx) = CommandSender::channel();
        let other = sender.clone();
        let before = Instant::now();
        sender.send(SchedulerCommand::Play).unwrap();
        let between = Instant::now();
        other.send(SchedulerCommand::Stop).unwrap();
        let after = Instant::now();
        let first = rx.recv().unwrap();
        let second = rx.recv().unwrap();
        assert!(matches!(first.command, SchedulerCommand::Play));
        assert!(matches!(second.command, SchedulerCommand::Stop));
        assert!((before..=between).contains(&first.enqueued_at));
        assert!((between..=after).contains(&second.enqueued_at));
        drop(rx);
        assert_eq!(
            sender.send(SchedulerCommand::Play).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
