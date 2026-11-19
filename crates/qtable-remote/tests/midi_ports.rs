//! Explicit opt-in: needs a running CoreMIDI/ALSA service, but no hardware.
#![cfg(unix)]
use midir::{MidiOutput, MidiOutputConnection, os::unix::VirtualOutput};
use qtable_remote::{Endpoint, Ingress, MidiPorts};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while !condition() {
        assert!(Instant::now() < deadline, "MIDI discovery timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}
// Use another process: creating an endpoint in the discovery process itself can
// refresh CoreMIDI's cache and hide missing notification/run-loop processing.
struct ExternalSource {
    child: Child,
    replies: mpsc::Receiver<String>,
}
impl ExternalSource {
    fn start(name: &str) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "virtual_source_child",
                "--ignored",
                "--nocapture",
            ])
            .env("QTABLE_TEST_MIDI_SOURCE", name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.starts_with("SOURCE:") && tx.send(line).is_err() {
                    break;
                }
            }
        });
        let source = Self { child, replies };
        source.reply();
        source
    }
    fn reply(&self) {
        assert_eq!(
            self.replies.recv_timeout(Duration::from_secs(6)).unwrap(),
            "SOURCE: ready"
        );
    }
    fn command(&mut self, command: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
        self.reply();
    }
}
impl Drop for ExternalSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "subprocess helper for virtual_source_hotplug_remote_switch_and_reconnect"]
fn virtual_source_child() {
    let Ok(name) = std::env::var("QTABLE_TEST_MIDI_SOURCE") else {
        return;
    };
    let source = |name: &str| -> MidiOutputConnection {
        MidiOutput::new("qtable external hotplug test")
            .unwrap()
            .create_virtual(name)
            .unwrap()
    };
    // An anchor lets the parent confirm its first discovery pass before hot-plug.
    let _anchor = source(&format!("{name}-anchor"));
    let mut output = None;
    let ready = || {
        println!("SOURCE: ready");
        std::io::stdout().flush().unwrap();
    };
    ready();
    for command in std::io::stdin().lock().lines() {
        match command.unwrap().as_str() {
            "plug" => output = Some(source(&name)),
            "unplug" => output = None,
            "cc-note" => {
                let output = output.as_mut().unwrap();
                output.send(&[0xb0, 1, 64]).unwrap();
                output.send(&[0x90, 60, 100]).unwrap();
            }
            "cc" => output.as_mut().unwrap().send(&[0xb0, 1, 127]).unwrap(),
            "note-off" => output.as_mut().unwrap().send(&[0x80, 60, 0]).unwrap(),
            command => panic!("unknown command {command}"),
        }
        ready();
    }
}

#[test]
#[ignore = "requires CoreMIDI/ALSA; run explicitly for MIDI acceptance"]
fn virtual_source_hotplug_remote_switch_and_reconnect() {
    let name = format!("qtable-hotplug-{}", std::process::id());
    let mut output = ExternalSource::start(&name);
    let ingress = Arc::new(Ingress::default());
    let ports = MidiPorts::start(ingress.clone(), BTreeMap::from([(name.clone(), true)])).unwrap();
    wait_for(|| {
        ports
            .snapshot()
            .ports
            .iter()
            .any(|p| p.name == format!("{name}-anchor"))
    });
    eprintln!(
        "PASS: initial snapshot before external plug: {:?}",
        ports.snapshot()
    );
    let started = Instant::now();
    output.command("plug");
    wait_for(|| {
        ports
            .snapshot()
            .ports
            .iter()
            .any(|p| p.name == name && p.connected)
    });
    eprintln!(
        "PASS: external plug connected after {:?}",
        started.elapsed()
    );
    output.command("cc-note");
    let mut events = Vec::new();
    wait_for(|| {
        events.extend(ingress.drain().events);
        events.len() >= 2
    });
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[0].endpoint, Endpoint::Midi { name: n, .. } if n == &name));
    assert!(events[0].sequence < events[1].sequence);
    eprintln!("PASS: virtual CC + note received in order");
    let started = Instant::now();
    output.command("unplug");
    wait_for(|| !ports.snapshot().ports.iter().any(|p| p.name == name));
    eprintln!("PASS: unplug discovered after {:?}", started.elapsed());
    let started = Instant::now();
    output.command("plug");
    wait_for(|| {
        ports
            .snapshot()
            .ports
            .iter()
            .any(|p| p.name == name && p.connected)
    });
    eprintln!(
        "PASS: replug reconnects by saved name after {:?}",
        started.elapsed()
    );
    ports.set_remote(name.clone(), false);
    wait_for(|| {
        ports
            .snapshot()
            .ports
            .iter()
            .any(|p| p.name == name && !p.remote && !p.connected)
    });
    let activity = ingress.drain().activity;
    output.command("cc");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(ingress.drain().activity, activity);
    eprintln!("PASS: Remote Off receives no messages");
    ports.set_remote(name.clone(), true);
    wait_for(|| {
        ports
            .snapshot()
            .ports
            .iter()
            .any(|p| p.name == name && p.connected)
    });
    output.command("note-off");
    wait_for(|| ingress.drain().activity > activity);
    eprintln!("PASS: Remote On resumes input");
}
