use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn play_status_quit_prints_project_tracks_and_exits_successfully() {
    // Bind an isolated loopback endpoint instead of using the example's ports.
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut project = qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/generic-receiver.ron"
    ))
    .unwrap();
    for destination in &mut project.outputs {
        destination.destination_mut().unwrap().host = "127.0.0.1".into();
        destination.destination_mut().unwrap().port = receiver.local_addr().unwrap().port();
        destination.destination_mut().unwrap().mode = qtable_engine::model::UdpMode::Unicast;
    }
    let path = std::env::temp_dir().join(format!("qtable-cli-smoke-{}.ron", std::process::id()));
    qtable_engine::file::save(&path, &project).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qtable"))
        .arg("play")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch qtable");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"status\nquit\n")
        .unwrap();
    let output = child.wait_with_output().expect("wait for qtable");
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Piped, there is no prompt and the banner goes to stderr, so stdout carries only output.
    assert!(!stdout.contains("> "), "stdout: {stdout}");
    assert!(!stdout.contains("help for commands"), "stdout: {stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("help for commands"), "stderr: {stderr}");
    assert!(stdout.starts_with("Transport: stopped"), "stdout: {stdout}");
    assert!(stdout.contains("Music:"), "stdout: {stdout}");
    assert!(stdout.contains("Lighting cue:"), "stdout: {stdout}");
}

/// Run `qtable play` on the example project with `input` on stdin; returns the exit status.
fn run_example(input: &[u8]) -> (std::process::ExitStatus, String) {
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut project = qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/generic-receiver.ron"
    ))
    .unwrap();
    for destination in &mut project.outputs {
        destination.destination_mut().unwrap().host = "127.0.0.1".into();
        destination.destination_mut().unwrap().port = receiver.local_addr().unwrap().port();
        destination.destination_mut().unwrap().mode = qtable_engine::model::UdpMode::Unicast;
    }
    let path = std::env::temp_dir().join(format!(
        "qtable-cli-exit-{}-{}.ron",
        std::process::id(),
        input.len()
    ));
    qtable_engine::file::save(&path, &project).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qtable"))
        .arg("play")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch qtable");
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().expect("wait for qtable");
    std::fs::remove_file(path).unwrap();
    (
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_failed_command_sets_a_non_zero_exit_status_at_end_of_input() {
    let (status, stderr) = run_example(b"status\nnonsense\n");
    assert_eq!(status.code(), Some(1), "stderr: {stderr}");
    assert!(stderr.contains("unknown command"), "stderr: {stderr}");
    let (status, stderr) = run_example(b"stop-all\nlaunch nosuchtrack 1\nquit\n");
    assert_eq!(status.code(), Some(1), "stderr: {stderr}");
    assert!(stderr.contains("unknown track"), "stderr: {stderr}");
    let (status, stderr) = run_example(b"stop-all\nstatus\nquit\n");
    assert_eq!(status.code(), Some(0), "stderr: {stderr}");
}

#[test]
fn version_and_help_print_and_exit_zero() {
    let qtable = |arg: &str| {
        Command::new(env!("CARGO_BIN_EXE_qtable"))
            .arg(arg)
            .output()
            .expect("run qtable")
    };
    for flag in ["--version", "-V"] {
        let output = qtable(flag);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("qtable {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
    for flag in ["--help", "-h"] {
        let output = qtable(flag);
        assert_eq!(output.status.code(), Some(0));
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("qtable play <project.ron>"), "{text}");
        assert!(text.contains("may change before"), "{text}");
    }
    let output = qtable("nonsense");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: qtable play"));
}
