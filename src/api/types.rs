use serde::{Deserialize, Serialize};

// ── T002: StreamEvent ─────────────────────────────────────────────

/// A single event emitted by the streaming conversation interface.
///
/// Mirrors the existing WebSocket protocol semantics and is designed
/// for consumption via flutter_rust_bridge (FRB).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// Incremental text from the model response.
    Chunk {
        /// UTF-8 text delta.
        delta: String,
    },

    /// The agent is invoking a tool.
    ToolCall {
        /// Tool name (e.g., "shell", "file_read").
        tool: String,
        /// JSON-encoded arguments.
        arguments: String,
    },

    /// A tool invocation has completed.
    ToolResult {
        /// Tool name matching a prior ToolCall.
        tool: String,
        /// Tool output text.
        output: String,
        /// Whether the tool succeeded.
        success: bool,
    },

    /// Agent has finished processing this message.
    Done {
        /// Full aggregated response text.
        full_response: String,
    },

    /// An error occurred during processing.
    Error {
        /// Human-readable error message.
        message: String,
    },
}

// ── T003: ApiError ────────────────────────────────────────────────

/// Errors returned by the public API surface.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ApiError {
    /// A configuration value failed validation.
    #[error("validation error: {message}")]
    ValidationError { message: String },

    /// The agent handle has not been initialized.
    #[error("agent not initialized")]
    NotInitialized,

    /// An internal error occurred.
    #[error("internal error: {message}")]
    Internal { message: String },

    /// A config file operation failed.
    #[error("config file error: {message}")]
    ConfigFileError { message: String },
}

// ── T004: ConfigPatch ─────────────────────────────────────────────

/// Partial configuration update for runtime changes.
///
/// All fields are optional — only `Some` values are applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigPatch {
    /// Provider name (e.g., "openai", "anthropic").
    pub provider: Option<String>,
    /// Model identifier (e.g., "gpt-4", "claude-3-opus").
    pub model: Option<String>,
    /// API key / token for the provider.
    pub api_key: Option<String>,
    /// API base URL override.
    pub api_base: Option<String>,
    /// Sampling temperature.
    pub temperature: Option<f64>,
    /// System prompt override.
    pub system_prompt: Option<String>,
    /// Maximum tool iterations per turn.
    pub max_tool_iterations: Option<usize>,
    /// Maximum conversation history length.
    pub max_history_messages: Option<usize>,
}

impl ConfigPatch {
    /// Validate patch fields. Returns `Err` with a description on failure.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(t) = self.temperature {
            if !(0.0..=2.0).contains(&t) {
                return Err(format!("temperature {t} out of range [0.0, 2.0]"));
            }
        }
        if let Some(ref key) = self.api_key {
            if key.is_empty() {
                return Err("api_key must not be empty".into());
            }
        }
        if let Some(max) = self.max_tool_iterations {
            if max == 0 {
                return Err("max_tool_iterations must be > 0".into());
            }
        }
        Ok(())
    }

    /// Apply this patch onto a mutable `Config`, overwriting only `Some` fields.
    pub fn apply_to(&self, config: &mut crate::config::Config) {
        if let Some(ref provider) = self.provider {
            config.default_provider = Some(provider.clone());
        }
        if let Some(ref model) = self.model {
            config.default_model = Some(model.clone());
        }
        if let Some(ref key) = self.api_key {
            config.api_key = Some(key.clone());
        }
        if let Some(ref url) = self.api_base {
            config.api_url = Some(url.clone());
        }
        if let Some(t) = self.temperature {
            config.default_temperature = t;
        }
        if let Some(ref prompt) = self.system_prompt {
            config.system_prompt = Some(prompt.clone());
        }
        if let Some(max) = self.max_tool_iterations {
            config.agent.max_tool_iterations = max;
        }
        if let Some(max) = self.max_history_messages {
            config.agent.max_history_messages = max;
        }
    }
}

// ── T005: ObserverEventDto ────────────────────────────────────────

/// FRB-compatible subset of `ObserverEvent`.
///
/// Uses only concrete, FRB-translatable types: no `Duration`, no trait objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObserverEventDto {
    AgentStart {
        provider: String,
        model: String,
    },
    LlmRequest {
        provider: String,
        model: String,
        messages_count: u32,
    },
    LlmResponse {
        provider: String,
        model: String,
        duration_ms: u64,
        success: bool,
        error_message: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    AgentEnd {
        provider: String,
        model: String,
        duration_ms: u64,
        tokens_used: Option<u64>,
        cost_usd: Option<f64>,
    },
    ToolCallStart {
        tool: String,
        arguments: Option<String>,
    },
    ToolCall {
        tool: String,
        duration_ms: u64,
        success: bool,
    },
    TurnComplete,
    ChannelMessage {
        channel: String,
        direction: String,
    },
    Error {
        component: String,
        message: String,
    },
}

impl ObserverEventDto {
    /// Convert from the internal `ObserverEvent`, returning `None` for
    /// internal-only events that should not be exposed to the host.
    pub fn from_observer_event(event: &crate::observability::ObserverEvent) -> Option<Self> {
        use crate::observability::ObserverEvent;
        match event {
            ObserverEvent::AgentStart { provider, model } => Some(Self::AgentStart {
                provider: provider.clone(),
                model: model.clone(),
            }),
            ObserverEvent::LlmRequest {
                provider,
                model,
                messages_count,
            } => Some(Self::LlmRequest {
                provider: provider.clone(),
                model: model.clone(),
                #[allow(clippy::cast_possible_truncation)]
                messages_count: *messages_count as u32,
            }),
            ObserverEvent::LlmResponse {
                provider,
                model,
                duration,
                success,
                error_message,
                input_tokens,
                output_tokens,
            } => Some(Self::LlmResponse {
                provider: provider.clone(),
                model: model.clone(),
                duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
                success: *success,
                error_message: error_message.clone(),
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
            }),
            ObserverEvent::AgentEnd {
                provider,
                model,
                duration,
                tokens_used,
                cost_usd,
            } => Some(Self::AgentEnd {
                provider: provider.clone(),
                model: model.clone(),
                duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
                tokens_used: *tokens_used,
                cost_usd: *cost_usd,
            }),
            ObserverEvent::ToolCallStart { tool, arguments } => Some(Self::ToolCallStart {
                tool: tool.clone(),
                arguments: arguments.clone(),
            }),
            ObserverEvent::ToolCall {
                tool,
                duration,
                success,
            } => Some(Self::ToolCall {
                tool: tool.clone(),
                duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
                success: *success,
            }),
            ObserverEvent::TurnComplete => Some(Self::TurnComplete),
            ObserverEvent::ChannelMessage { channel, direction } => Some(Self::ChannelMessage {
                channel: channel.clone(),
                direction: direction.clone(),
            }),
            ObserverEvent::Error { component, message } => Some(Self::Error {
                component: component.clone(),
                message: message.clone(),
            }),
            // Internal-only events — filter out
            ObserverEvent::HeartbeatTick
            | ObserverEvent::CacheHit { .. }
            | ObserverEvent::CacheMiss { .. }
            | ObserverEvent::HandStarted { .. }
            | ObserverEvent::HandCompleted { .. }
            | ObserverEvent::HandFailed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T059: ConfigPatch partial merge ──

    #[test]
    fn config_patch_apply_to_overwrites_only_some_fields() {
        let mut config = crate::config::Config::default();
        let original_temp = config.default_temperature;

        let patch = ConfigPatch {
            provider: Some("anthropic".into()),
            model: Some("claude-3-opus".into()),
            api_key: None,
            api_base: None,
            temperature: None,
            system_prompt: None,
            max_tool_iterations: Some(5),
            max_history_messages: None,
        };

        patch.apply_to(&mut config);
        assert_eq!(config.default_provider.as_deref(), Some("anthropic"));
        assert_eq!(config.default_model.as_deref(), Some("claude-3-opus"));
        assert_eq!(config.agent.max_tool_iterations, 5);
        // Fields that were None should not have changed
        assert!(config.api_key.is_none());
        assert_eq!(config.default_temperature, original_temp);
    }

    #[test]
    fn config_patch_apply_to_all_fields() {
        let mut config = crate::config::Config::default();

        let patch = ConfigPatch {
            provider: Some("openai".into()),
            model: Some("gpt-4".into()),
            api_key: Some("sk-test".into()),
            api_base: Some("https://example.com".into()),
            temperature: Some(0.5),
            system_prompt: None,
            max_tool_iterations: Some(10),
            max_history_messages: Some(50),
        };

        patch.apply_to(&mut config);
        assert_eq!(config.default_provider.as_deref(), Some("openai"));
        assert_eq!(config.default_model.as_deref(), Some("gpt-4"));
        assert_eq!(config.api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.api_url.as_deref(), Some("https://example.com"));
        assert!((config.default_temperature - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.agent.max_tool_iterations, 10);
        assert_eq!(config.agent.max_history_messages, 50);
    }

    // ── T060: Validation rejects invalid values ──

    #[test]
    fn config_patch_validate_rejects_out_of_range_temperature() {
        let patch = ConfigPatch {
            temperature: Some(3.0),
            ..Default::default()
        };
        assert!(patch.validate().is_err());
    }

    #[test]
    fn config_patch_validate_rejects_empty_api_key() {
        let patch = ConfigPatch {
            api_key: Some(String::new()),
            ..Default::default()
        };
        assert!(patch.validate().is_err());
    }

    #[test]
    fn config_patch_validate_rejects_zero_max_tool_iterations() {
        let patch = ConfigPatch {
            max_tool_iterations: Some(0),
            ..Default::default()
        };
        assert!(patch.validate().is_err());
    }

    #[test]
    fn config_patch_validate_accepts_valid_patch() {
        let patch = ConfigPatch {
            provider: Some("openai".into()),
            temperature: Some(1.0),
            api_key: Some("sk-valid".into()),
            max_tool_iterations: Some(5),
            ..Default::default()
        };
        assert!(patch.validate().is_ok());
    }

    #[test]
    fn config_patch_validate_accepts_empty_patch() {
        let patch = ConfigPatch::default();
        assert!(patch.validate().is_ok());
    }

    // ── T047: ObserverEventDto conversion filters internal events ──

    #[test]
    fn observer_event_dto_filters_heartbeat() {
        let event = crate::observability::ObserverEvent::HeartbeatTick;
        assert!(ObserverEventDto::from_observer_event(&event).is_none());
    }

    #[test]
    fn observer_event_dto_converts_llm_request() {
        let event = crate::observability::ObserverEvent::LlmRequest {
            provider: "openai".into(),
            model: "gpt-4".into(),
            messages_count: 3,
        };
        let dto = ObserverEventDto::from_observer_event(&event).unwrap();
        assert!(
            matches!(dto, ObserverEventDto::LlmRequest { provider, model, messages_count } if provider == "openai" && model == "gpt-4" && messages_count == 3)
        );
    }

    #[test]
    fn observer_event_dto_converts_turn_complete() {
        let event = crate::observability::ObserverEvent::TurnComplete;
        let dto = ObserverEventDto::from_observer_event(&event).unwrap();
        assert!(matches!(dto, ObserverEventDto::TurnComplete));
    }
}
