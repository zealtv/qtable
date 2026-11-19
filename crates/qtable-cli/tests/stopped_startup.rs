use qtable_engine::model::*;
use std::io::{BufRead, BufReader, Write};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

fn startup_timing_limit() -> Duration {
    if std::env::var_os("QTABLE_ASSERT_SEND").is_some_and(|value| value == "1") {
        Duration::from_secs(1)
    } else {
        // Four bars at 120 BPM take eight seconds. Allow scheduling stalls
        // under build/test load (3.5 s seen on 2026-10-11 with two
        // concurrent full checks) while still rejecting a launch on that grid.
        Duration::from_secs(6)
    }
}

struct Cli {
    child: Child,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    path: PathBuf,
    socket: UdpSocket,
}

impl Cli {
    fn open(name: &str) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(startup_timing_limit()))
            .unwrap();
        let mut project = Project {
            tempo: Tempo::new(120.0).unwrap(),
            global_quantisation: GlobalQuantisation::Value(QuantisationValue::Bars(4)),
            ..Project::default()
        };
        project.outputs.push(Track::new_output(
            TrackId(201),
            "test",
            "127.0.0.1",
            socket.local_addr().unwrap().port(),
        ));
        let mut clip = Clip::new(
            ClipId(1),
            "first",
            BeatTime(3840),
            qtable_engine::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap();
        let mut track = Track::new(TrackId(1), "test", 1);
        let startup = OscAddress::Absolute("/startup".into());
        clip.events
            .push(track.hit(BeatTime::ZERO, startup, vec![], || DeviceId(1)));
        track.route = Route::single(TrackId(201));
        track.slots[0].clip = Some(clip);
        project.tracks.push(track);
        project.scenes.push(Scene {
            id: SceneId(1),
            name: "first".into(),
            follow_actions: SceneFollowActions::default(),
        });
        let path =
            std::env::temp_dir().join(format!("qtable-cli-{name}-{}.ron", std::process::id()));
        qtable_engine::file::save(&path, &project).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_qtable"))
            .arg("play")
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        // Piped stdout carries only command output, with no prompt, so read it by line.
        let (tx, lines) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            reader: Some(reader),
            path,
            socket,
        }
    }

    /// Send a command. Commands other than `status` print nothing, so there is nothing to
    /// wait for; `status` is the synchronising call.
    fn send(&mut self, command: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
    }

    /// Run `status` and return its text, up to and including the `test:` track line, which
    /// closes the output for this one-track project.
    fn status(&mut self) -> String {
        // Process readiness is not the launch-to-OSC timing assertion. A
        // concurrent workspace build can delay the child or stdout reader.
        let timeout = if startup_timing_limit() == Duration::from_secs(1) {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(10)
        };
        self.send("status");
        let mut text = String::new();
        loop {
            let line = self.lines.recv_timeout(timeout).expect("CLI status line");
            text.push_str(&line);
            text.push('\n');
            if line.starts_with("test:") {
                return text;
            }
        }
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn opening_then_launching_a_clip_or_scene_starts_at_zero_under_four_bar_quantisation() {
    for (name, command) in [("clip", "launch test 1"), ("scene", "scene 1")] {
        let mut cli = Cli::open(name);
        assert!(cli.status().contains("Transport: stopped"));
        let started = Instant::now();
        cli.send(command);
        let mut packet = [0; 1024];
        let count = cli.socket.recv(&mut packet).expect(
            "beat-zero OSC event must arrive immediately, not at the 8-second grid boundary",
        );
        let elapsed = started.elapsed();
        let limit = startup_timing_limit();
        eprintln!("{name} startup: {elapsed:?} (limit {limit:?})");
        assert!(
            elapsed < limit,
            "{name} startup took {elapsed:?} (limit {limit:?})"
        );
        assert!(packet[..count].starts_with(b"/startup\0"));
        let status = cli.status();
        assert!(status.contains("Transport: playing"), "{status}");
        assert!(status.contains("first (slot 1"), "{status}");
        assert!(!status.contains("triggered:"), "{status}");
    }
}

#[test]
fn play_explicitly_starts_transport_without_launching_a_clip() {
    let mut cli = Cli::open("explicit-play");
    assert!(cli.status().contains("Transport: stopped"));
    cli.send("start-transport");
    let limit = Instant::now() + Duration::from_secs(1);
    loop {
        let status = cli.status();
        if status.contains("Transport: playing") {
            assert!(status.contains("test: —"), "{status}");
            break;
        }
        assert!(Instant::now() < limit, "play did not start transport");
        std::thread::sleep(Duration::from_millis(1));
    }
}
