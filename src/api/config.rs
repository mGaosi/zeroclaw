use crate::api::types::{ApiError, ConfigPatch};
use crate::config::Config;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

/// Manages runtime configuration with live reload support.
pub struct RuntimeConfigManager {
    /// Current configuration (protected for concurrent access).
    config: Arc<Mutex<Config>>,
    /// Change notification channel — subsystems subscribe to this.
    tx: watch::Sender<Config>,
    /// Receiver kept alive so the channel never closes.
    _rx: watch::Receiver<Config>,
    /// File path for persistence (optional — may be None on Android).
    config_path: Option<PathBuf>,
}

impl RuntimeConfigManager {
    /// Create a new config manager from the initial config and optional file path.
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let (tx, rx) = watch::channel(config.clone());
        Self {
            config: Arc::new(Mutex::new(config)),
            tx,
            _rx: rx,
            config_path,
        }
    }

    /// Get a clone of the current config.
    pub async fn get_config(&self) -> Config {
        self.config.lock().await.clone()
    }

    /// Subscribe to configuration changes.
    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.tx.subscribe()
    }

    /// Apply a partial configuration update.
    ///
    /// Validates the patch, merges into current config, optionally saves to disk,
    /// and notifies all subscribers.
    pub async fn update_config(&self, patch: ConfigPatch) -> Result<(), ApiError> {
        patch
            .validate()
            .map_err(|msg| ApiError::ValidationError { message: msg })?;

        // Validate workspace_dir change before applying.
        if let Some(ref dir) = patch.workspace_dir {
            let path = std::path::Path::new(dir);
            tokio::fs::create_dir_all(path)
                .await
                .map_err(|e| ApiError::ValidationError {
                    message: format!("failed to create workspace directory '{dir}': {e}"),
                })?;
            let probe = path.join(".zeroclaw_write_probe");
            tokio::fs::write(&probe, b"probe")
                .await
                .map_err(|e| ApiError::ValidationError {
                    message: format!("workspace directory '{dir}' is not writable: {e}"),
                })?;
            let _ = tokio::fs::remove_file(&probe).await;
        }

        let mut config = self.config.lock().await;
        let mut new_config = config.clone();
        patch.apply_to(&mut new_config);

        // Save to disk if path is set
        if let Some(ref path) = self.config_path {
            let toml_str = toml::to_string_pretty(&new_config).map_err(|e| ApiError::Internal {
                message: format!("failed to serialize config: {e}"),
            })?;
            tokio::fs::write(path, toml_str)
                .await
                .map_err(|e| ApiError::ConfigFileError {
                    message: format!("failed to write config file: {e}"),
                })?;
        }

        *config = new_config.clone();
        let _ = self.tx.send(new_config);
        Ok(())
    }

    /// Reload config from the TOML file on disk.
    pub async fn reload_from_file(&self) -> Result<(), ApiError> {
        let path = self.config_path.as_ref().ok_or(ApiError::ConfigFileError {
            message: "no config file path set at initialization".into(),
        })?;

        let content =
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| ApiError::ConfigFileError {
                    message: format!("failed to read config file: {e}"),
                })?;

        let file_config: Config =
            toml::from_str(&content).map_err(|e| ApiError::ConfigFileError {
                message: format!("failed to parse config file: {e}"),
            })?;

        let mut config = self.config.lock().await;
        *config = file_config.clone();
        let _ = self.tx.send(file_config);
        Ok(())
    }
}

/// Get the current runtime configuration as a JSON string.
pub fn get_config(handle: &crate::api::lifecycle::AgentHandle) -> Result<String, ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    let config_manager = handle.config_manager();
    let config = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(config_manager.get_config())
    });
    serde_json::to_string(&config).map_err(|e| ApiError::Internal {
        message: format!("failed to serialize config: {e}"),
    })
}

/// Apply a partial configuration update at runtime.
pub async fn update_config(
    handle: &crate::api::lifecycle::AgentHandle,
    patch: ConfigPatch,
) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    handle.config_manager().update_config(patch).await
}

/// Reload configuration from the TOML file.
pub async fn reload_config_from_file(
    handle: &crate::api::lifecycle::AgentHandle,
) -> Result<(), ApiError> {
    if !handle.is_initialized() {
        return Err(ApiError::NotInitialized);
    }
    handle.config_manager().reload_from_file().await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T059 supplement: RuntimeConfigManager update/get ──

    #[tokio::test]
    async fn config_manager_update_and_get() {
        let config = Config::default();
        let mgr = RuntimeConfigManager::new(config, None);

        let patch = ConfigPatch {
            provider: Some("anthropic".into()),
            ..Default::default()
        };
        mgr.update_config(patch).await.unwrap();

        let updated = mgr.get_config().await;
        assert_eq!(updated.default_provider.as_deref(), Some("anthropic"));
    }

    // ── T060: update_config rejects invalid patches ──

    #[tokio::test]
    async fn config_manager_rejects_invalid_temperature() {
        let config = Config::default();
        let mgr = RuntimeConfigManager::new(config, None);

        let patch = ConfigPatch {
            temperature: Some(5.0),
            ..Default::default()
        };
        let result = mgr.update_config(patch).await;
        assert!(matches!(result, Err(ApiError::ValidationError { .. })));
    }

    #[tokio::test]
    async fn config_manager_rejects_empty_api_key() {
        let config = Config::default();
        let mgr = RuntimeConfigManager::new(config, None);

        let patch = ConfigPatch {
            api_key: Some(String::new()),
            ..Default::default()
        };
        let result = mgr.update_config(patch).await;
        assert!(matches!(result, Err(ApiError::ValidationError { .. })));
    }

    #[tokio::test]
    async fn config_manager_rejects_zero_max_tool_iterations() {
        let config = Config::default();
        let mgr = RuntimeConfigManager::new(config, None);

        let patch = ConfigPatch {
            max_tool_iterations: Some(0),
            ..Default::default()
        };
        let result = mgr.update_config(patch).await;
        assert!(matches!(result, Err(ApiError::ValidationError { .. })));
    }

    // ── T061: Subscriber receives config changes ──

    #[tokio::test]
    async fn config_manager_subscriber_notified_on_change() {
        let config = Config::default();
        let mgr = RuntimeConfigManager::new(config, None);
        let mut rx = mgr.subscribe();

        let patch = ConfigPatch {
            model: Some("new-model".into()),
            ..Default::default()
        };
        mgr.update_config(patch).await.unwrap();

        assert!(rx.has_changed().unwrap());
        let new_config = rx.borrow_and_update().clone();
        assert_eq!(new_config.default_model.as_deref(), Some("new-model"));
    }

    // ── T062: Secrets injection via ConfigPatch ──

    #[tokio::test]
    async fn config_manager_secrets_via_config_patch() {
        let config = Config::default();
        assert!(config.api_key.is_none());

        let mgr = RuntimeConfigManager::new(config, None);
        let patch = ConfigPatch {
            api_key: Some("sk-secret-key".into()),
            ..Default::default()
        };
        mgr.update_config(patch).await.unwrap();

        let updated = mgr.get_config().await;
        assert_eq!(updated.api_key.as_deref(), Some("sk-secret-key"));
    }

    // ── T031: reload_from_file with no path returns error ──

    #[tokio::test]
    async fn config_manager_reload_without_path_errors() {
        let config = Config::default();
        let mgr = RuntimeConfigManager::new(config, None);
        let result = mgr.reload_from_file().await;
        assert!(matches!(result, Err(ApiError::ConfigFileError { .. })));
    }
}
