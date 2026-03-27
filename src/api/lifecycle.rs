use crate::agent::agent::Agent;
use crate::api::config::RuntimeConfigManager;
use crate::api::host_tools::HostToolRegistry;
use crate::api::observer::ObserverCallbackRegistry;
use crate::api::types::{ApiError, ConfigPatch};
use crate::channels::session_backend::SessionBackend;
use crate::observability::MultiObserver;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
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
    host_tool_registry: Arc<HostToolRegistry>,
    cancel_token: CancellationToken,
    config_rx: Arc<Mutex<tokio::sync::watch::Receiver<crate::config::Config>>>,
    initialized: bool,
    session_backend: Arc<RwLock<Option<Arc<dyn SessionBackend>>>>,
    current_session_key: Arc<Mutex<Option<String>>>,
}

impl AgentHandle {
    /// Check if the handle is properly initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get a reference to the agent.
    pub(crate) fn agent(&self) -> Arc<Mutex<Agent>> {
        self.agent.clone()
    }

    /// Get a reference to the config manager.
    pub(crate) fn config_manager(&self) -> Arc<RuntimeConfigManager> {
        self.config_manager.clone()
    }

    /// Get a reference to the observer registry.
    pub(crate) fn observer_registry(&self) -> Arc<ObserverCallbackRegistry> {
        self.observer_registry.clone()
    }

    /// Get a reference to the host tool registry.
    pub(crate) fn host_tool_registry(&self) -> Arc<HostToolRegistry> {
        self.host_tool_registry.clone()
    }

    /// Get the cancellation token.
    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Cancel any in-flight processing and reset the token for next use.
    pub fn cancel_and_reset(&self) {
        self.cancel_token.cancel();
    }

    /// Get a reference to the config watch receiver for change detection.
    pub(crate) fn config_rx(
        &self,
    ) -> Arc<Mutex<tokio::sync::watch::Receiver<crate::config::Config>>> {
        self.config_rx.clone()
    }

    /// Get a reference to the session backend.
    pub fn session_backend(&self) -> Arc<RwLock<Option<Arc<dyn SessionBackend>>>> {
        self.session_backend.clone()
    }

    /// Get a reference to the current session key tracker.
    pub fn current_session_key(&self) -> Arc<Mutex<Option<String>>> {
        self.current_session_key.clone()
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

    /// Test-only constructor: build an AgentHandle from a pre-built Agent
    /// and optional config file path. Used in integration tests with mock providers.
    #[doc(hidden)]
    pub fn from_agent_for_test(
        agent: crate::agent::agent::Agent,
        config: crate::config::Config,
        config_path: Option<std::path::PathBuf>,
    ) -> Self {
        let observer_registry = Arc::new(ObserverCallbackRegistry::new());
        let config_manager = Arc::new(RuntimeConfigManager::new(config, config_path));
        let config_rx = config_manager.subscribe();
        config_rx.has_changed().ok();
        let host_tool_registry = Arc::new(HostToolRegistry::new());
        Self {
            agent: Arc::new(Mutex::new(agent)),
            config_manager,
            observer_registry,
            host_tool_registry,
            cancel_token: CancellationToken::new(),
            config_rx: Arc::new(Mutex::new(config_rx)),
            initialized: true,
            session_backend: Arc::new(RwLock::new(None)),
            current_session_key: Arc::new(Mutex::new(None)),
        }
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

    // Validate workspace directory: ensure it exists and is writable.
    let workspace_dir = &config.workspace_dir;
    if !workspace_dir.as_os_str().is_empty() {
        tokio::fs::create_dir_all(workspace_dir)
            .await
            .map_err(|e| ApiError::ValidationError {
                message: format!(
                    "failed to create workspace directory '{}': {e}",
                    workspace_dir.display()
                ),
            })?;

        // Verify write permissions by creating and removing a temp file.
        let probe = workspace_dir.join(".zeroclaw_write_probe");
        tokio::fs::write(&probe, b"probe")
            .await
            .map_err(|e| ApiError::ValidationError {
                message: format!(
                    "workspace directory '{}' is not writable: {e}",
                    workspace_dir.display()
                ),
            })?;
        let _ = tokio::fs::remove_file(&probe).await;
    }

    // Build the agent
    let mut agent = Agent::from_config(&config)
        .await
        .map_err(|e| ApiError::Internal {
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

    // Initialize session backend if persistence is enabled.
    let session_backend: Option<Arc<dyn SessionBackend>> = if config
        .channels_config
        .session_persistence
        && !config.workspace_dir.as_os_str().is_empty()
    {
        let backend_type = &config.channels_config.session_backend;
        if backend_type == "sqlite" {
            match crate::channels::session_sqlite::SqliteSessionBackend::new(&config.workspace_dir)
            {
                Ok(b) => Some(Arc::new(b)),
                Err(e) => {
                    tracing::warn!("Failed to initialize SQLite session backend: {e}");
                    None
                }
            }
        } else {
            match crate::channels::session_store::SessionStore::new(&config.workspace_dir) {
                Ok(b) => Some(Arc::new(b)),
                Err(e) => {
                    tracing::warn!("Failed to initialize JSONL session backend: {e}");
                    None
                }
            }
        }
    } else {
        None
    };

    let config_path_buf = config_path.map(std::path::PathBuf::from);
    let config_manager = Arc::new(RuntimeConfigManager::new(config, config_path_buf));
    let config_rx = config_manager.subscribe();
    // Mark initial value as seen so first has_changed() returns false
    config_rx.has_changed().ok();
    let cancel_token = CancellationToken::new();
    let host_tool_registry = Arc::new(HostToolRegistry::new());

    Ok(AgentHandle {
        agent: Arc::new(Mutex::new(agent)),
        config_manager,
        observer_registry,
        host_tool_registry,
        cancel_token,
        config_rx: Arc::new(Mutex::new(config_rx)),
        initialized: true,
        session_backend: Arc::new(RwLock::new(session_backend)),
        current_session_key: Arc::new(Mutex::new(None)),
    })
}

/// Shutdown the agent instance, cancelling any in-flight work.
pub fn shutdown(handle: AgentHandle) -> Result<(), ApiError> {
    handle.cancel_token.cancel();
    // Drop the handle — Arc references will clean up when all tasks complete.
    drop(handle);
    Ok(())
}
