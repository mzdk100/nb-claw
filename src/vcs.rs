//! Version Control System (VCS) integration for nb-claw
//!
//! Provides automatic file version tracking using git2 library.
//! Features:
//! - Automatic file path detection from Python code and CMD commands
//! - Git-based version control for any file on the system
//! - Snapshot management (create, list, restore)
//! - Python module for model-controlled version operations

mod engine;
mod path_extractor;
mod py_module;

pub use engine::VcsEngine;
pub use path_extractor::extract_paths;
pub use py_module::create_vcs_module;
