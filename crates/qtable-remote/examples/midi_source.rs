//! Virtual source for unattended MIDI-in captures (CoreMIDI or ALSA).
//! cargo run -p qtable-remote --example midi_source -- 30
#[cfg(unix)]
fn main() {
    use midir::{MidiOutput, os::unix::VirtualOutput};
    use std::time::{Duration, Instant};
    let seconds = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let mut output = MidiOutput::new("qtable MIDI test source")
        .unwrap()
        .create_virtual("qtable-input-probe")
        .unwrap();
    eprintln!("qtable-input-probe ready");
    let start = Instant::now();
    let mut value = 0;
    while start.elapsed() < Duration::from_secs(seconds) {
        output.send(&[0xb0, 1, value]).unwrap();
        output.send(&[0x90, 60, 100]).unwrap();
        output.send(&[0x80, 60, 0]).unwrap();
        value = (value + 1) % 128;
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[cfg(not(unix))]
fn main() {
    eprintln!("The virtual source probe requires CoreMIDI or ALSA.");
}
