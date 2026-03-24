//! Helpers for building `AgentHandle` instances with mock providers for
//! API integration tests.

use std::sync::Arc;
use zeroclaw::agent::agent::Agent;
use zeroclaw::agent::dispatcher::NativeToolDispatcher;
use zeroclaw::api::lifecycle::AgentHandle;
use zeroclaw::api::types::ConfigPatch;
use zeroclaw::config::{Config, MemoryConfig};
use zeroclaw::memory;
use zeroclaw::observability::NoopObserver;
use zeroclaw::providers::{ChatResponse, Provider};
use zeroclaw::tools::Tool;

/// Build a test `AgentHandle` with a mock provider and no tools.
pub fn build_test_handle(provider: Box<dyn Provider>) -> AgentHandle {
    build_test_handle_with_tools(provider, vec![])
}

/// Build a test `AgentHandle` with a mock provider and specified tools.
pub fn build_test_handle_with_tools(
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
) -> AgentHandle {
    build_test_handle_with_config(provider, tools, Config::default(), None)
}

/// Build a test `AgentHandle` with full control over config and file path.
pub fn build_test_handle_with_config(
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    config: Config,
    config_path: Option<std::path::PathBuf>,
) -> AgentHandle {
    let mem_cfg = MemoryConfig {
        backend: "none".into(),
        ..MemoryConfig::default()
    };
    let memory = Arc::from(memory::create_memory(&mem_cfg, &std::env::temp_dir(), None).unwrap());
    let agent = Agent::builder()
        .provider(provider)
        .tools(tools)
        .memory(memory)
        .observer(Arc::from(NoopObserver {}) as Arc<dyn zeroclaw::observability::Observer>)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .build()
        .unwrap();

    AgentHandle::from_agent_for_test(agent, config, config_path)
}

/// Create a simple text-only `ChatResponse`.
pub fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        usage: None,
        reasoning_content: None,
    }
}

/// Collect all `StreamEvent`s from a channel until it closes or a terminal event.
pub async fn collect_events(
    mut rx: tokio::sync::mpsc::Receiver<zeroclaw::api::types::StreamEvent>,
) -> Vec<zeroclaw::api::types::StreamEvent> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let is_terminal = matches!(
            event,
            zeroclaw::api::types::StreamEvent::Done { .. }
                | zeroclaw::api::types::StreamEvent::Error { .. }
        );
        events.push(event);
        if is_terminal {
            break;
        }
    }
    events
}
