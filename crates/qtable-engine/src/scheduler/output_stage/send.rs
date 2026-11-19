//! Frame transmission, bundle packing and reporting what left the socket. A frame's packets can
//! go one at a time (`InFlight`), so the scheduler can send other outputs' hits between them.
use std::collections::VecDeque;
use std::io;
use std::time::Instant;

use rosc::{OscMessage, OscPacket};

use super::bundle;
use super::throughput::Counts;
use crate::activity::Activity;
use crate::model::{Track, TrackId};
use crate::scheduler::events::EventSink;
use crate::scheduler::{FrameProfile, SchedulerEvent};
use crate::sender::{OscSender, SendTiming};

/// Where a stage's parameter messages go, and what they report.
pub(super) struct Out<'a> {
    pub(super) output: &'a Track,
    pub(super) sender: &'a mut OscSender,
    pub(super) activity: &'a Activity,
    pub(super) tx: &'a mut EventSink,
    pub(super) diagnostics: bool,
    pub(super) live_timing: bool,
    pub(super) now: Instant,
    pub(super) due: Instant,
    /// Diagnostics: when the exact jump behind the next send began and finished slot lookup.
    pub(super) jump: Option<(Instant, Instant)>,
    /// Diagnostics: the last pass that held this grid frame back for a cluster.
    pub(super) withheld: Option<Instant>,
    /// Leave the frame's packets queued for `Stage::send_next` instead of sending them now.
    pub(super) resumable: bool,
}

/// Diagnostic marks taken while a frame prepares its messages.
#[derive(Clone, Copy)]
pub(super) struct Phases {
    /// The pass that let the frame go.
    pub(super) released: Instant,
    pub(super) withheld: Option<Instant>,
    pub(super) began: Instant,
    pub(super) prepared: Instant,
    pub(super) built: Instant,
    pub(super) slots: usize,
    pub(super) considered: usize,
    pub(super) sampled: usize,
    pub(super) merged: u64,
}

/// One frame's packed packets, waiting to leave in order, with what its report needs.
pub(super) struct InFlight {
    /// Each packet with the parameter track behind each message it carries.
    packets: VecDeque<(OscPacket, Vec<TrackId>)>,
    due: Instant,
    /// When dispatch began this frame; activity is recorded at this instant.
    entry: Instant,
    jump: Option<(Instant, Instant)>,
    phases: Option<Phases>,
    timing: SendTiming,
    messages: usize,
}

impl InFlight {
    /// Pack a frame's messages, one per datagram or into bundles under the MTU rule. `tracks`
    /// names the parameter track behind each message. None if packing failed (reported).
    pub(super) fn new(
        out: &mut Out,
        counts: &mut Counts,
        messages: Vec<OscMessage>,
        bundle: bool,
        tracks: &[TrackId],
        phases: Option<Phases>,
    ) -> Option<Self> {
        let packed = if messages.is_empty() {
            Vec::new()
        } else {
            pack(out.tx, counts, messages, bundle)?
        };
        let mut index = 0;
        let packets = packed
            .into_iter()
            .map(|packet| {
                let carried = match &packet {
                    OscPacket::Bundle(bundle) => bundle.content.len(),
                    OscPacket::Message(_) => 1,
                };
                index += carried;
                (packet, tracks[index - carried..index].to_vec())
            })
            .collect();
        Some(Self {
            packets,
            due: out.due,
            entry: out.now,
            jump: out.jump.take(),
            phases,
            timing: SendTiming::default(),
            messages: 0,
        })
    }

    pub(super) fn due(&self) -> Instant {
        self.due
    }

    /// Send the next packet, recording what left; once none remain, report the frame and return
    /// true.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn send_next(
        &mut self,
        sender: &mut OscSender,
        output: &Track,
        activity: &Activity,
        tx: &mut EventSink,
        counts: &mut Counts,
        diagnostics: bool,
        live_timing: bool,
    ) -> bool {
        if let Some((packet, tracks)) = self.packets.pop_front() {
            let timing = self.phases.is_some().then_some(&mut self.timing);
            match sender.send_timed(output, &packet, timing) {
                Ok(bytes) if bytes > 0 => {
                    counts.packets += 1;
                    counts.messages += tracks.len() as u64;
                    counts.bytes += bytes as u64;
                    self.messages += tracks.len();
                    for track in tracks {
                        activity.record(output.id, Some(track), self.entry);
                    }
                }
                Ok(_) => {}
                Err(error) => tx.send(SchedulerEvent::Error(error)),
            }
            if !self.packets.is_empty() {
                return false;
            }
        }
        if diagnostics {
            // For live diagnostics, completion bounds the departure of every packet in
            // this frame, including the last packet of a multi-parameter jump.
            let send_at = if live_timing {
                Instant::now()
            } else {
                self.entry
            };
            let due = self.due;
            let after = |at: Instant| at.saturating_duration_since(due);
            let jump = self.jump;
            let timing = self.timing;
            let profile = self
                .phases
                .map_or_else(FrameProfile::default, |phases| FrameProfile {
                    jump: jump.map(|(began, found)| (after(began), after(found))),
                    released: after(phases.released),
                    withheld_last: phases.withheld.map(after),
                    began: after(phases.began),
                    prepared: after(phases.prepared),
                    built: after(phases.built),
                    send: timing,
                    slots: phases.slots,
                    considered: phases.considered,
                    sampled: phases.sampled,
                    merged: phases.merged,
                });
            tx.send(SchedulerEvent::FrameSent {
                output: output.id,
                due,
                lateness: after(send_at),
                entry_lateness: after(self.entry),
                messages: self.messages,
                profile,
            });
        }
        true
    }
}

/// Messages as plain datagrams, or packed into bundles under the MTU rule. None (reported) if
/// packing failed.
fn pack(
    tx: &mut EventSink,
    counts: &mut Counts,
    messages: Vec<OscMessage>,
    bundle: bool,
) -> Option<Vec<OscPacket>> {
    if !bundle {
        return Some(messages.into_iter().map(OscPacket::Message).collect());
    }
    match bundle::pack(messages, bundle::MTU) {
        Ok(packed) => {
            counts.oversize += packed.oversize as u64;
            Some(packed.packets)
        }
        Err(error) => {
            tx.send(SchedulerEvent::Error(io::Error::new(
                io::ErrorKind::InvalidInput,
                error,
            )));
            None
        }
    }
}

/// Send messages as plain datagrams, or packed into bundles under the MTU rule, counting what
/// left. `sent` gets the index of each message whose datagram the sender accepted (an inactive
/// output's zero-byte send included) and the instant just before that datagram's send call;
/// a failed send reports its error instead.
#[allow(clippy::too_many_arguments)]
pub(super) fn transmit(
    sender: &mut OscSender,
    output: &Track,
    tx: &mut EventSink,
    counts: &mut Counts,
    messages: Vec<OscMessage>,
    bundle: bool,
    mut timing: Option<&mut SendTiming>,
    mut sent: impl FnMut(usize, Instant),
) {
    if messages.is_empty() {
        return;
    }
    let Some(packets) = pack(tx, counts, messages, bundle) else {
        return;
    };
    let mut index = 0;
    for packet in packets {
        let carried = match &packet {
            OscPacket::Bundle(bundle) => bundle.content.len(),
            OscPacket::Message(_) => 1,
        };
        let before = Instant::now();
        match sender.send_timed(output, &packet, timing.as_deref_mut()) {
            Ok(bytes) => {
                if bytes > 0 {
                    counts.packets += 1;
                    counts.messages += carried as u64;
                    counts.bytes += bytes as u64;
                }
                (index..index + carried).for_each(|message| sent(message, before));
            }
            Err(error) => tx.send(SchedulerEvent::Error(error)),
        }
        index += carried;
    }
}
