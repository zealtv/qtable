//! Headless qtable engine.
//!
//! The project model, file format, clock, scheduler, and command layer live
//! here. Clients drive playback through the engine; they never send OSC directly.

pub mod activity;
pub mod clock;
pub mod edit;
pub mod file;
pub mod model;

pub mod resolver;

pub mod sender;

pub mod scheduler;

pub mod engine;
mod history;

pub mod status;

mod supervise;
