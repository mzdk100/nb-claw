//! Memory management module
//!
//! Provides advanced memory management capabilities with Python bindings.

pub mod engine;
pub mod py_module;

pub use engine::Memory;
pub use py_module::create_memory_module;
