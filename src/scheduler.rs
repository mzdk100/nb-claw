//! Intelligent Task Scheduler for nb-claw
//!
//! Provides a pure-programming task scheduler that allows the AI to:
//! - Create scheduled tasks (one-time or recurring)
//! - Query and manage existing tasks
//! - Execute tasks automatically in the background
//!
//! Features:
//! - Multiple schedule types: once, interval, cron-like
//! - Task persistence (survives restarts)
//! - Task execution with Python code or shell commands
//! - Automatic retry on failure
//! - Task status tracking and history

mod engine;
mod py_module;

pub use engine::{SchedulerEngine, TaskEvent};
pub use py_module::create_scheduler_module;
