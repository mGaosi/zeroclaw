use crate::api::types::ObserverEventDto;
use crate::observability::traits::ObserverMetric;
use crate::observability::{Observer, ObserverEvent};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Registry of host-provided observer callbacks.
///
/// Implements the `Observer` trait so it can be plugged into the existing
/// observability pipeline.
pub struct ObserverCallbackRegistry {
    next_id: Mutex<u64>,
    senders: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<ObserverEventDto>>>>,
}

impl ObserverCallbackRegistry {
    pub fn new() -> Self {
        Self {
            next_id: Mutex::new(1),
            senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a new observer callback. Returns the observer ID.
    pub fn register(&self, sender: mpsc::UnboundedSender<ObserverEventDto>) -> u64 {
        let mut id_guard = self.next_id.lock();
        let id = *id_guard;
        *id_guard += 1;
        self.senders.lock().insert(id, sender);
        id
    }

    /// Unregister an observer by ID.
    pub fn unregister(&self, id: u64) {
        self.senders.lock().remove(&id);
    }
}

impl Observer for ObserverCallbackRegistry {
    fn record_event(&self, event: &ObserverEvent) {
        if let Some(dto) = ObserverEventDto::from_observer_event(event) {
            let senders = self.senders.lock();
            for sender in senders.values() {
                let _ = sender.send(dto.clone());
            }
        }
    }

    fn record_metric(&self, _metric: &ObserverMetric) {
        // Metrics are not forwarded to host callbacks
    }

    fn name(&self) -> &str {
        "callback-registry"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Register an observer callback. Returns the observer ID.
pub fn register_observer(
    handle: &crate::api::lifecycle::AgentHandle,
    sender: mpsc::UnboundedSender<ObserverEventDto>,
) -> Result<u64, crate::api::types::ApiError> {
    if !handle.is_initialized() {
        return Err(crate::api::types::ApiError::NotInitialized);
    }
    Ok(handle.observer_registry().register(sender))
}

/// Unregister an observer callback by ID.
pub fn unregister_observer(
    handle: &crate::api::lifecycle::AgentHandle,
    observer_id: u64,
) -> Result<(), crate::api::types::ApiError> {
    if !handle.is_initialized() {
        return Err(crate::api::types::ApiError::NotInitialized);
    }
    handle.observer_registry().unregister(observer_id);
    Ok(())
}

// ── FRB StreamSink wrappers ──────────────────────────────────────

/// FRB-compatible observer registration: accepts a `StreamSink<ObserverEventDto>`
/// that FRB translates into a Dart `Stream<ObserverEventDto>`.
///
/// Internally bridges to `register_observer()` via an `mpsc::unbounded_channel`,
/// forwarding events from the channel into the sink.
/// Returns the observer ID for later unregistration.
#[cfg(feature = "frb")]
pub fn register_observer_stream(
    handle: &crate::api::lifecycle::AgentHandle,
    sink: flutter_rust_bridge::StreamSink<ObserverEventDto>,
) -> Result<u64, crate::api::types::ApiError> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ObserverEventDto>();
    let id = register_observer(handle, tx)?;

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if sink.add(event).is_err() {
                break;
            }
        }
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T067: Observer registration and event delivery ──

    #[test]
    fn observer_register_and_receive_events() {
        let registry = ObserverCallbackRegistry::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let id = registry.register(tx);
        assert!(id > 0);

        // Trigger an event via the Observer trait
        registry.record_event(&ObserverEvent::TurnComplete);

        let dto = rx.try_recv().unwrap();
        assert!(matches!(dto, ObserverEventDto::TurnComplete));
    }

    #[test]
    fn observer_delivers_llm_request_event() {
        let registry = ObserverCallbackRegistry::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        registry.register(tx);

        registry.record_event(&ObserverEvent::LlmRequest {
            provider: "test".into(),
            model: "gpt-4".into(),
            messages_count: 5,
        });

        let dto = rx.try_recv().unwrap();
        assert!(matches!(
            dto,
            ObserverEventDto::LlmRequest {
                provider,
                model,
                messages_count
            } if provider == "test" && model == "gpt-4" && messages_count == 5
        ));
    }

    #[test]
    fn observer_filters_internal_events() {
        let registry = ObserverCallbackRegistry::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        registry.register(tx);

        registry.record_event(&ObserverEvent::HeartbeatTick);
        assert!(rx.try_recv().is_err());
    }

    // ── T068: Observer unregistration stops delivery ──

    #[test]
    fn observer_unregister_stops_events() {
        let registry = ObserverCallbackRegistry::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let id = registry.register(tx);

        // Deliver before unregister
        registry.record_event(&ObserverEvent::TurnComplete);
        assert!(rx.try_recv().is_ok());

        // Unregister
        registry.unregister(id);

        // No more deliveries
        registry.record_event(&ObserverEvent::TurnComplete);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn observer_multiple_registrations() {
        let registry = ObserverCallbackRegistry::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        let id1 = registry.register(tx1);
        let _id2 = registry.register(tx2);

        registry.record_event(&ObserverEvent::TurnComplete);
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());

        // Unregister first — second still gets events
        registry.unregister(id1);
        registry.record_event(&ObserverEvent::TurnComplete);
        assert!(rx1.try_recv().is_err());
        assert!(rx2.try_recv().is_ok());
    }
}
