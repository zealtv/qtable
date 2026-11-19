//! UDP OSC capture for integration tests, independent of the engine.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rosc::{OscMessage, OscPacket};

/// Local monotonic arrival time and decoded message.
pub type ReceivedMessage = (Instant, OscMessage);

/// Captures OSC datagrams on a background thread until dropped.
///
/// Bundle timetags are not scheduled: all messages in a datagram share its arrival
/// timestamp and are flattened depth-first in wire order. Times describe local
/// socket reads, not the sender's clock or kernel receive timestamps.
pub struct TestReceiver {
    addr: SocketAddr,
    messages: Receiver<io::Result<ReceivedMessage>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl TestReceiver {
    /// Bind an ephemeral IPv4 loopback port.
    pub fn bind() -> io::Result<Self> {
        Self::bind_addr("127.0.0.1:0")
    }

    /// Bind an explicit address, e.g. `0.0.0.0:0` to receive IPv4 broadcasts.
    /// Senders must enable broadcast and use a broadcast IP with `addr().port()`.
    pub fn bind_addr(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        let addr = socket.local_addr()?;
        socket.set_read_timeout(Some(Duration::from_millis(20)))?;
        let (tx, messages) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("qtable-test-receiver".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                if std::env::var_os("QTABLE_TEST_RECEIVER_DISABLE_QOS").is_none() {
                    // Measurement hygiene: timestamps are taken after recv_from,
                    // so receiver scheduling otherwise adds unrelated jitter.
                    // SAFETY: sets only this worker's QoS with valid arguments.
                    let result = unsafe {
                        libc::pthread_set_qos_class_self_np(
                            libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
                            0,
                        )
                    };
                    if result != 0 {
                        eprintln!("qtable test receiver: USER_INTERACTIVE QoS failed: {result}");
                    }
                }
                let mut buffer = vec![0; 65_535];
                while !worker_stop.load(Ordering::Relaxed) {
                    match socket.recv_from(&mut buffer) {
                        Ok((len, _)) => {
                            let arrived = Instant::now();
                            match rosc::decoder::decode_udp(&buffer[..len]) {
                                Ok(([], packet)) => {
                                    let mut flattened = Vec::new();
                                    flatten(packet, arrived, &mut flattened);
                                    for message in flattened {
                                        if tx.send(Ok(message)).is_err() {
                                            return;
                                        }
                                    }
                                }
                                result => {
                                    let error = io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        format!("invalid OSC datagram: {result:?}"),
                                    );
                                    if tx.send(Err(error)).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock
                                    | io::ErrorKind::TimedOut
                                    | io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) => {
                            let _ = tx.send(Err(error));
                            return;
                        }
                    }
                }
            })?;
        Ok(Self {
            addr,
            messages,
            stop,
            worker: Some(worker),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Consume exactly `count` messages, waiting at most `timeout` in total.
    ///
    /// Excess messages remain queued. A timeout reports how many were consumed;
    /// that partial batch is discarded. Malformed datagrams surface as InvalidData.
    pub fn wait_for_messages(
        &self,
        count: usize,
        timeout: Duration,
    ) -> io::Result<Vec<ReceivedMessage>> {
        let started = Instant::now();
        let mut messages = Vec::new();
        for _ in 0..count {
            match self
                .messages
                .recv_timeout(timeout.saturating_sub(started.elapsed()))
            {
                Ok(message) => messages.push(message?),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("received {} of {count} OSC messages", messages.len()),
                    ));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "OSC receiver thread stopped",
                    ));
                }
            }
        }
        Ok(messages)
    }
}

impl Drop for TestReceiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn flatten(packet: OscPacket, arrived: Instant, messages: &mut Vec<ReceivedMessage>) {
    match packet {
        OscPacket::Message(message) => messages.push((arrived, message)),
        OscPacket::Bundle(bundle) => {
            for packet in bundle.content {
                flatten(packet, arrived, messages);
            }
        }
    }
}
