//! LLM client management and tool definitions
//!
//! This module provides integration with various LLM providers
//! and defines the tools that the AI assistant can use.

mod client;
mod tools;

pub use {client::LlmManager, tools::ToolRegistry};
