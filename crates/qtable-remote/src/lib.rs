//! External input, independent of any host toolkit. The owner drains ordered input and
//! executes returned commands. Adapters never own playback or send OSC.
pub mod ingress;
pub mod keys;
pub mod mapping;
pub mod midi;
pub mod router;
mod takeover;
pub mod translate;

/// Interpreted intent; the original envelope remains with the router's consumers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Absolute(f64),
    Steps(i32),
    Press,
    Release,
}

pub use ingress::{Drain, Endpoint, Envelope, Ingress, Input, Overflow, QueueClass};
pub use mapping::MappingResolver;
pub use midi::{MidiPorts, Port, PortSnapshot};
pub use router::{Action, Consumer, Received, Router, ViewAction};
