//! Address resolution, typed OSC packets, and synchronous UDP output.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use rosc::{OscMessage, OscPacket, OscType};

pub use crate::model::resolve_address;
use crate::model::{Event, OscArgument, Track, TrackId};

/// Build one immediate OSC message; timing belongs to the scheduler.
pub fn event_packet(event: &Event, resolved_address: &str) -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: resolved_address.to_owned(),
        args: event
            .arguments
            .iter()
            .map(|argument| match argument {
                OscArgument::Int(value) => OscType::Int(*value),
                OscArgument::Float(value) => OscType::Float(*value),
                OscArgument::String(value) => OscType::String(value.clone()),
                OscArgument::Bool(value) => OscType::Bool(*value),
            })
            .collect(),
    })
}

/// A socket bound once, with resolved endpoints retained for its lifetime.
///
/// Each entry remembers the host and port it was resolved for, so editing a
/// output cannot reuse its former endpoint. The resolver thread re-looks-up cached hostnames on a slow timer
/// and calls `insert_resolved` again when an address changes.
/// `send()` never resolves; endpoints arrive through `prepare`, `resolve_literal`
/// or `insert_resolved`.
/// No threads or receiver-specific timing protocols are involved.
struct ResolvedEndpoint {
    host: String,
    port: u16,
    address: SocketAddr,
}

pub struct OscSender {
    socket: UdpSocket,
    outputs: HashMap<TrackId, ResolvedEndpoint>,
}

impl OscSender {
    /// Bind an ephemeral IPv4 socket, supporting unicast and broadcast.
    pub fn new() -> io::Result<Self> {
        Self::bind("0.0.0.0:0")
    }

    /// Bind a chosen interface/address family once. Use `[::]:0` for IPv6.
    /// Outputs must resolve to the bound socket's address family.
    pub fn bind(address: impl ToSocketAddrs) -> io::Result<Self> {
        let socket = UdpSocket::bind(address)?;
        // SO_BROADCAST only permits broadcast destinations and leaves unicast
        // sends alone, so one setting at bind serves a mixed fan-out without
        // a setsockopt per packet.
        socket.set_broadcast(true)?;
        Ok(Self {
            socket,
            outputs: HashMap::new(),
        })
    }

    /// Resolve and cache an output's endpoint, blocking on DNS if needed.
    /// Never call this from the scheduler or UI thread; the resolver thread and
    /// tests use it. `send()` only uses endpoints already cached.
    pub fn prepare(&mut self, output: &Track) -> io::Result<SocketAddr> {
        if let Some(address) = self.cached(output) {
            return Ok(address);
        }
        let destination = output
            .destination()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "track is not an output"))?;
        let ipv4 = self.socket.local_addr()?.is_ipv4();
        let address = (destination.host.as_str(), destination.port)
            .to_socket_addrs()?
            .find(|address| address.is_ipv4() == ipv4)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "output has no address matching the sender socket family",
                )
            })?;
        self.insert_resolved(output, address);
        Ok(address)
    }

    fn cached(&self, output: &Track) -> Option<SocketAddr> {
        let destination = output.destination()?;
        // Keyed by id so the per-send lookup allocates nothing.
        self.outputs
            .get(&output.id)
            .filter(|entry| entry.host == destination.host && entry.port == destination.port)
            .map(|entry| entry.address)
    }

    /// Whether `send()` has an endpoint for this output.
    pub fn is_resolved(&self, output: &Track) -> bool {
        self.cached(output).is_some()
    }

    /// Cache the endpoint when the host is an IP literal of the socket's family.
    /// This involves no DNS, so it is safe on any thread. Returns whether the
    /// output is now resolved.
    pub fn resolve_literal(&mut self, output: &Track) -> bool {
        if self.is_resolved(output) {
            return true;
        }
        let Some(destination) = output.destination() else {
            return false;
        };
        let Ok(ip) = destination.host.parse::<std::net::IpAddr>() else {
            return false;
        };
        let ipv4 = self.socket.local_addr().is_ok_and(|a| a.is_ipv4());
        if ip.is_ipv4() != ipv4 {
            return false;
        }
        self.insert_resolved(output, SocketAddr::new(ip, destination.port));
        true
    }

    /// Cache an endpoint resolved elsewhere (the resolver thread).
    pub fn insert_resolved(&mut self, output: &Track, address: SocketAddr) {
        let Some(destination) = output.destination() else {
            return;
        };
        self.outputs.insert(
            output.id,
            ResolvedEndpoint {
                host: destination.host.clone(),
                port: destination.port,
                address,
            },
        );
    }

    /// Encode and send a packet, returning the datagram's byte count.
    /// Never resolves: an output without a cached endpoint fails with
    /// `AddrNotAvailable`, so the realtime thread cannot block on DNS.
    /// Encoding and UDP errors propagate to the scheduler.
    pub fn send(&mut self, output: &Track, packet: &OscPacket) -> io::Result<usize> {
        self.send_timed(output, packet, None)
    }

    /// `send`, adding the time spent encoding and in the socket call to `timing`.
    pub(crate) fn send_timed(
        &mut self,
        output: &Track,
        packet: &OscPacket,
        timing: Option<&mut SendTiming>,
    ) -> io::Result<usize> {
        if !output.mixer.active {
            return Ok(0);
        }
        let began = timing.is_some().then(Instant::now);
        let bytes = rosc::encoder::encode(packet)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let address = self.cached(output).ok_or_else(|| {
            io::Error::new(io::ErrorKind::AddrNotAvailable, "output is unresolved")
        })?;
        let encoded = began.map(|_| Instant::now());
        let sent = self.socket.send_to(&bytes, address);
        if let (Some(timing), Some(began), Some(encoded)) = (timing, began, encoded) {
            timing.encode += encoded - began;
            timing.socket += encoded.elapsed();
            timing.packets += 1;
        }
        sent
    }
}

/// Diagnostic totals across a frame's packets. `socket` ends when `send_to` returns,
/// which bounds submission to the kernel, not network departure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SendTiming {
    /// Encoding plus endpoint lookup.
    pub encode: Duration,
    pub socket: Duration,
    pub packets: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OscAddress, UdpMode};

    #[test]
    fn prepared_send_reuses_cache_and_serves_both_modes() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut output = Track::new_output(
            TrackId(201),
            "cached",
            "127.0.0.1",
            receiver.local_addr().unwrap().port(),
        );
        let mut sender = OscSender::new().unwrap();
        let address = sender.prepare(&output).unwrap();
        assert_eq!(address, receiver.local_addr().unwrap());
        // An unresolvable host can only send successfully through the cache.
        output.destination_mut().unwrap().host = "cached.invalid".into();
        sender.insert_resolved(&output, address);
        let packet = OscPacket::Message(OscMessage {
            addr: "/test".into(),
            args: vec![],
        });
        for mode in [
            UdpMode::Unicast,
            UdpMode::Broadcast,
            UdpMode::Broadcast,
            UdpMode::Unicast,
        ] {
            output.destination_mut().unwrap().mode = mode;
            assert_eq!(sender.send(&output, &packet).unwrap(), 12);
            // Set once at bind; sends never change it.
            assert!(sender.socket.broadcast().unwrap());
        }
    }

    #[test]
    fn editing_a_output_does_not_reuse_its_former_endpoint() {
        let mut sender = OscSender::new().unwrap();
        let mut output = Track::new_output(TrackId(201), "moved", "127.0.0.1", 9000);
        assert!(sender.resolve_literal(&output));
        output.destination_mut().unwrap().port = 9001;
        assert!(!sender.is_resolved(&output));
        output.destination_mut().unwrap().port = 9000;
        output.destination_mut().unwrap().host = "127.0.0.2".into();
        assert!(!sender.is_resolved(&output));
        assert!(sender.resolve_literal(&output));
        assert!(sender.is_resolved(&output));
    }

    #[test]
    fn resolves_boundary_slashes_and_root() {
        for (prefix, suffix, expected) in [
            ("", "/play", "/play"),
            ("", "play", "/play"),
            ("/main", "/play", "/main/play"),
            ("/main/", "/play", "/main/play"),
            ("/main", "play", "/main/play"),
            ("/main/", "play", "/main/play"),
            ("/", "/play", "/play"),
            ("", "/", "/"),
            ("/", "/", "/"),
            ("/main", "/", "/main/"),
            ("/main", "/a//b", "/main/a//b"),
        ] {
            assert_eq!(
                resolve_address(prefix, &OscAddress::Relative(suffix.into())),
                expected
            );
        }
    }

    #[test]
    fn absolute_override_ignores_prefix() {
        for prefix in ["", "/main", "/main/", "/"] {
            for address in ["/other/play", "/"] {
                assert_eq!(
                    resolve_address(prefix, &OscAddress::Absolute(address.into())),
                    address
                );
            }
        }
    }
}
