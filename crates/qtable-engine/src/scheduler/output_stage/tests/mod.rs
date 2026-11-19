mod budget;
mod bundle;
mod deferral;
mod fields;
mod frames;
mod reporting;
mod sent;

use super::*;
use crate::model::{Route, SendMode};
use crate::scheduler::parameter_controls::controls;
use rosc::{OscMessage, OscType};
use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;

fn project(mode: SendMode, delay_ms: f64) -> Project {
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.route = Route::single(TrackId(2));
    track.delay_ms = delay_ms;
    track.mixer_device.volume.output.enabled = true;
    track.mixer_device.volume.output.send_mode = mode;
    Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(TrackId(2), "Out", "127.0.0.1", 9000)],
        ..Project::default()
    }
}

fn ms(anchor: Instant, ms: u64) -> Instant {
    anchor + Duration::from_millis(ms)
}

/// A frame at Frame Rate 50 Hz, so ticks fall on whole 20 ms steps from the anchor.
fn at_50_hz(mut p: Project) -> Project {
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 50.0;
    p
}

struct Rig {
    stages: OutputStages,
    anchor: Instant,
    receiver: UdpSocket,
    sender: OscSender,
    activity: Activity,
}

impl Rig {
    fn new(p: &Project) -> Self {
        let anchor = Instant::now();
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        let mut sender = OscSender::new().unwrap();
        sender.insert_resolved(&p.outputs[0], receiver.local_addr().unwrap());
        Self {
            stages: OutputStages::new(p, controls(p), anchor),
            anchor,
            receiver,
            sender,
            activity: Activity::default(),
        }
    }

    fn update(&mut self, p: &Project, at: u64) {
        self.stages.update(p, controls(p), ms(self.anchor, at));
    }

    fn deadline(&self, p: &Project) -> Option<Instant> {
        self.stages.deadline(p, &self.sender)
    }

    /// Dispatch at a synthetic instant and read every datagram sent, including none.
    fn datagrams(&mut self, p: &Project, at: u64) -> Vec<OscPacket> {
        self.datagrams_before(p, at, None)
    }

    /// As `datagrams`, with the next clip event due at `clip`.
    fn datagrams_before(&mut self, p: &Project, at: u64, clip: Option<Instant>) -> Vec<OscPacket> {
        let (tx, _rx) = mpsc::sync_channel(100);
        self.stages.dispatch(
            p,
            &mut self.sender,
            &self.activity,
            &mut EventSink::new(tx),
            &clear(ms(self.anchor, at), clip),
        );
        self.read()
    }

    fn read(&mut self) -> Vec<OscPacket> {
        let mut packets = vec![];
        let mut buffer = [0; 2048];
        while let Ok(size) = self.receiver.recv(&mut buffer) {
            let (_, packet) = rosc::decoder::decode_udp(&buffer[..size]).unwrap();
            packets.push(packet);
        }
        packets
    }

    /// Dispatch at a synthetic instant and read every value sent, including none.
    fn values(&mut self, p: &Project, at: u64) -> Vec<f32> {
        self.values_before(p, at, None)
    }

    /// As `values`, with the next clip event due at `clip`.
    fn values_before(&mut self, p: &Project, at: u64, clip: Option<Instant>) -> Vec<f32> {
        self.datagrams_before(p, at, clip)
            .into_iter()
            .map(|packet| {
                let OscPacket::Message(message) = packet else {
                    panic!("expected a message");
                };
                assert_eq!(message.addr, "/volume");
                value(&message)
            })
            .collect()
    }
}

/// The chain at `now` with one clip event due at `clip`, or none, and the plan seeing far ahead.
fn clear(now: Instant, clip: Option<Instant>) -> cluster::Chain {
    cluster::Chain::summarise(
        clip.into_iter(),
        std::iter::empty(),
        now + Duration::from_secs(60),
        now,
    )
}

fn value(message: &OscMessage) -> f32 {
    let [OscType::Float(value)] = message.args.as_slice() else {
        panic!("expected a float");
    };
    *value
}

fn set_volume(p: &mut Project, value: f64) {
    p.tracks[0].mixer_device.volume.value = value;
}

/// `count` tracks, each routed to the output with its own volume (0.1, 0.2, ...), 50 Hz frames.
fn many(count: u32, mode: SendMode) -> Project {
    let mut p = at_50_hz(project(mode, 0.0));
    p.tracks.clear();
    for id in 1..=u64::from(count) {
        let mut track = Track::new(TrackId(id), "Track", 0);
        track.route = Route::single(TrackId(100));
        track.mixer_device.volume.output.enabled = true;
        track.mixer_device.volume.output.send_mode = mode;
        track.mixer_device.volume.value = id as f64 / 10.0;
        p.tracks.push(track);
    }
    p.outputs[0].id = TrackId(100);
    p
}

fn with_stage(mut p: Project, edit: impl FnOnce(&mut crate::model::OutputStage)) -> Project {
    edit(&mut p.outputs[0].output.as_mut().unwrap().stage);
    p
}

fn volumes(p: &mut Project, value: f64) {
    for track in &mut p.tracks {
        track.mixer_device.volume.value = value + track.id.0 as f64 / 100.0;
    }
}

fn event_packet(value: f32) -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: "/event".into(),
        args: vec![OscType::Float(value)],
    })
}

fn send_event(rig: &mut Rig, p: &Project, value: f32, at: u64) {
    let (tx, _rx) = mpsc::sync_channel(100);
    let at = ms(rig.anchor, at);
    rig.stages
        .event(
            &mut rig.sender,
            &mut EventSink::new(tx),
            &p.outputs[0],
            &event_packet(value),
            at,
            at,
        )
        .unwrap();
}
