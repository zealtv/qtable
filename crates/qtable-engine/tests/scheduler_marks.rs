//! Diagnostics-only phase marks (launch-timing-investigation, 2026-10-20): a launch from stopped
//! reports where its command's time went, and other diagnostics report nothing new.
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

fn labels_after_launch(marks: bool) -> Vec<&'static str> {
    let receiver = TestReceiver::bind().unwrap();
    let project = fixture::project(receiver.addr().port(), &[0.0]);
    let scene = project.scenes[0].id;
    let lookahead = Duration::from_millis(5);
    let handle = if marks {
        scheduler::spawn_with_marks(project, lookahead)
    } else {
        scheduler::spawn_with_diagnostics(project, lookahead, true)
    }
    .unwrap();
    handle.send(SchedulerCommand::LaunchScene(scene)).unwrap();
    let mut labels = Vec::new();
    let mut commands_applied = 0;
    let until = Instant::now() + Duration::from_millis(300);
    while let Ok(event) = handle
        .events
        .recv_timeout(until.saturating_duration_since(Instant::now()))
    {
        match event {
            SchedulerEvent::Marks(marks) => labels.extend(marks.into_iter().map(|(l, _)| l)),
            SchedulerEvent::CommandApplied { .. } => commands_applied += 1,
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    assert_eq!(commands_applied > 0, marks);
    labels
}

#[test]
fn a_launch_marks_its_command_sync_and_transport_start_in_order() {
    let labels = labels_after_launch(true);
    // The worker's own startup sync comes first; read from the command on.
    let start = labels.iter().position(|l| *l == "command.arrived").unwrap();
    let labels = &labels[start..];
    let position = |label: &str| {
        labels
            .iter()
            .position(|l| *l == label)
            .unwrap_or_else(|| panic!("no {label} in {labels:?}"))
    };
    let order = [
        "command.arrived",
        "command.begin",
        "command.planned",
        "sync.begin",
        "sync.automation",
        "sync.end",
        "start.end",
        "command.end",
    ];
    for pair in order.windows(2) {
        assert!(
            position(pair[0]) < position(pair[1]),
            "{pair:?} in {labels:?}"
        );
    }
    assert!(labels.contains(&"plan.end"));
}

#[test]
fn diagnostics_alone_record_no_marks() {
    assert!(labels_after_launch(false).is_empty());
}
