//! Musical planning and the sole playback UDP thread.
use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rosc::OscPacket;

use crate::clock::{InternalClock, TempoClock};
use crate::model::{
    BeatTime, ClipId, FollowAction, GlobalQuantisation, LaunchQuantisation, Project, SceneId,
    Tempo, TrackId,
};
use crate::resolver::{ReportGate, Resolver, unresolved_error};
use crate::sender::{OscSender, event_packet};
use crate::supervise::report_panic;

pub(crate) use events::EventSink;
use follow::LaunchOrigin;
use helpers::due_at;
pub use output_stage::throughput::Throughput;
use scene_follow::SCENE_FOLLOW_BYPASS_GLOBAL;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

mod automation;
mod jumps;
pub use automation::{AutomationState, AutomationValue};
mod catch_up;
#[cfg(test)]
mod catch_up_tests;
mod command_queue;
mod commands;
pub(crate) use command_queue::CommandSender;
use command_queue::Envelope;
pub use commands::admission::AdmissionOutcome;
#[cfg(test)]
mod commands_tests;
mod events;
mod launch;
#[cfg(feature = "dev-hooks")]
mod marks;
// Without the feature the marks are no-ops, so the worker's call sites stay as they are.
#[cfg(not(feature = "dev-hooks"))]
#[path = "marks_off.rs"]
mod marks;
mod measurements;
#[cfg(feature = "dev-hooks")]
pub use measurements::{spawn_with_grid_lead, spawn_with_marks};
mod oneshots;
mod output_stage;
mod packets;
mod parameter_controls;
mod planner;
mod published;
mod reaper;
mod turns;
mod wait;
mod worker;
pub use planner::{DueEvent, PlannedAction, Planner};
use planner::{PendingStop, Voice};
pub use published::{ClipPosition, LanePosition, PublishedState, PublishedTrack};
pub use worker::{
    FrameProfile, SchedulerCommand, SchedulerEvent, SchedulerHandle, spawn, spawn_with_diagnostics,
};

#[cfg(target_os = "macos")]
mod realtime;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod legato_tests;
#[cfg(test)]
mod quantisation_tests;

#[cfg(test)]
mod scene_tests;

#[cfg(test)]
mod group_tests;

#[cfg(test)]
mod stopping_tests;

#[cfg(test)]
mod backlog_turn_tests;
#[cfg(test)]
mod cluster_deferral_tests;
#[cfg(test)]
mod cluster_sim;
#[cfg(test)]
mod cluster_tests;
mod follow;
mod follow_choice;
#[cfg(test)]
mod follow_queue_tests;
#[cfg(test)]
mod follow_stop_tests;
#[cfg(test)]
mod follow_tests;
#[cfg(test)]
mod held_frame_tests;
#[cfg(test)]
mod horizon_tests;
mod late;
#[cfg(test)]
mod overdue_follow_tests;
mod rebase;
#[cfg(test)]
mod rebase_send_path_tests;
#[cfg(test)]
mod rebase_tests;

mod scene_follow;
#[cfg(test)]
mod scene_follow_tests;

#[cfg(test)]
mod follow_grid_tests;

#[cfg(test)]
mod plan_cost_tests;

mod silence;
#[cfg(test)]
mod silence_tests;

mod publish;

mod delay;

mod helpers;

#[cfg(test)]
mod mixer_tests;
#[cfg(test)]
mod output_tests;

#[cfg(test)]
mod tail_tests;
mod tails;
mod timeline;
use timeline::{Region, Timeline};
mod transport;
#[cfg(test)]
mod valve_tests;

#[cfg(test)]
mod markers_tests;

#[cfg(test)]
mod unlinked_tests;
