//! Python interpreter integration for nb-claw
//!
//! This module provides an embedded Python interpreter that allows
//! the LLM to execute Python scripts to control the device.

mod interpreter;

pub use interpreter::*;
use pyo3::IntoPyObject;

pub trait Module<'a>: IntoPyObject<'a> {
    fn get_name() -> &'static str;
}
