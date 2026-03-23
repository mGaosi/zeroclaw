// T001: Public API module for ZeroClaw library interface.
//
// This module provides the Rust-native API surface consumed by
// flutter_rust_bridge (FRB) for code generation.

pub mod config;
pub mod conversation;
pub mod lifecycle;
pub mod observer;
pub mod types;

pub use config::RuntimeConfigManager;
pub use lifecycle::AgentHandle;
pub use observer::ObserverCallbackRegistry;
pub use types::{ApiError, ConfigPatch, ObserverEventDto, StreamEvent};

// FRB StreamSink wrappers (available only with the `frb` feature)
#[cfg(feature = "frb")]
pub use conversation::send_message_stream;
#[cfg(feature = "frb")]
pub use observer::register_observer_stream;
