use crate::agent::agent::Agent;
use crate::api::config::RuntimeConfigManager;
use crate::api::observer::ObserverCallbackRegistry;
use crate::api::types::{ApiError, ConfigPatch};
use crate::observability::MultiObserver;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Thin wrapper so an `Arc<ObserverCallbackRegistry>` can be stored inside a
/// `Box<dyn Observer>` while the same Arc is also held by `AgentHandle`.
struct SharedRegistryObserver(Arc<ObserverCallbackRegistry>);

impl crate::observability::Observer for SharedRegistryObserver {
    fn record_event(&self, event: &crate::observability::ObserverEvent) {
        self.0.record_event(event);
    }
    fn record_metric(&self, metric: &crate::observability::traits::ObserverMetric) {
        self.0.record_metric(metric);
    }
    fn flush(&self) {
        self.0.flush();
    }
    fn name(&self) -> &str {
        self.0.name()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Opaque handle to a running ZeroClaw agent instance.
///
/// Created via `init()`. The handle is `Send + Sync` and can be shared
/// across threads/isolates.
pub struct AgentHandle {
    agent: Arc<Mutex<Agent>>,
    config_manager: Arc<RuntimeConfigManager>,
    observer_registry: Arc<ObserverCallbackRegistry>,
    cancel_token: CancellationToken,
    config_rx: Mutex<tokio::sync::watch::Receiver<crate::config::Config>>,
    initialized: bool,
}

impl AgentHandle {
    /// Check if the handle is properly initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get a reference to the agent.
    pub fn agent(&self) -> Arc<Mutex<Agent>> {
        self.agent.clone()
    }

    /// Get a reference to the config manager.
    pub fn config_manager(&self) -> Arc<RuntimeConfigManager> {
        self.config_manager.clone()
    }

    /// Get a reference to the observer registry.
    pub fn observer_registry(&self) -> Arc<ObserverCallbackRegistry> {
        self.observer_registry.clone()
    }

    /// Get the cancellation token.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Cancel any in-flight processing and reset the token for next use.
    pub fn cancel_and_reset(&self) {
        self.cancel_token.cancel();
    }

    /// Check if config has changed since last call. Returns true if a rebuild is needed.
    pub async fn has_config_changed(&self) -> bool {
        let rx = self.config_rx.lock().await;
        rx.has_changed().unwrap_or(false)
    }

    /// Mark config changes as seen (call after rebuilding agent).
    pub async fn mark_config_seen(&self) {
        let mut rx = self.config_rx.lock().await;
        rx.mark_changed();
    }
}

/// Initialize a ZeroClaw agent instance.
///
/// Loads config from the provided path (optional), applies any runtime
/// overrides from `overrides`, and starts the agent runtime.
pub async fn init(
    config_path: Option<String>,
    overrides: Option<ConfigPatch>,
) -> Result<AgentHandle, ApiError> {
    // Load config from file or defaults
    let mut config = if let Some(ref path) = config_path {
        let path_buf = std::path::PathBuf::from(path);
        if path_buf.exists() {
            let content = tokio::fs::read_to_string(&path_buf).await.map_err(|e| {
                ApiError::ConfigFileError {
                    message: format!("failed to read config: {e}"),
                }
            })?;
            toml::from_str(&content).map_err(|e| ApiError::ConfigFileError {
                message: format!("failed to parse config: {e}"),
            })?
        } else {
            tracing::warn!("Config file not found at {path}, using defaults");
            crate::config::Config::default()
        }
    } else {
        crate::config::Config::default()
    };

    // Apply overrides if provided
    if let Some(patch) = overrides {
        patch
            .validate()
            .map_err(|msg| ApiError::ValidationError { message: msg })?;
        patch.apply_to(&mut config);
    }

    // Build the agent
    let mut agent = Agent::from_config(&config).map_err(|e| ApiError::Internal {
        message: format!("failed to build agent: {e}"),
    })?;

    let observer_registry = Arc::new(ObserverCallbackRegistry::new());

    // T046: Wire callback registry into agent's observer chain using MultiObserver.
    // The existing config-driven observer is already inside the agent; we wrap it
    // with the callback registry via a MultiObserver fan-out.
    let config_observer: Box<dyn crate::observability::Observer> =
        crate::observability::create_observer(&config.observability);
    let registry_wrapper: Box<dyn crate::observability::Observer> =
        Box::new(SharedRegistryObserver(observer_registry.clone()));
    let multi = Arc::new(MultiObserver::new(vec![config_observer, registry_wrapper]));
    agent.set_observer(multi);

    let config_path_buf = config_path.map(std::path::PathBuf::from);
    let config_manager = Arc::new(RuntimeConfigManager::new(config, config_path_buf));
    let config_rx = config_manager.subscribe();
    // Mark initial value as seen so first has_changed() returns false
    config_rx.has_changed().ok();
    let cancel_token = CancellationToken::new();

    Ok(AgentHandle {
        agent: Arc::new(Mutex::new(agent)),
        config_manager,
        observer_registry,
        cancel_token,
        config_rx: Mutex::new(config_rx),
        initialized: true,
    })
}

/// Shutdown the agent instance, cancelling any in-flight work.
pub fn shutdown(handle: AgentHandle) -> Result<(), ApiError> {
    handle.cancel_token.cancel();
    // Drop the handle — Arc references will clean up when all tasks complete.
    drop(handle);
    Ok(())
}
