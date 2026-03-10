//! Memory management module
//!
//! Provides advanced memory management capabilities with Python bindings.

pub mod engine;
pub mod manager;

pub use engine::Memory;
pub use manager::create_memory_module;
