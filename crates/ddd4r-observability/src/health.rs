//! Thread-safe liveness and readiness registry.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ObservabilityError, ObservabilityResult};

/// Health state of one runtime component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// The component operates normally.
    Up,
    /// The component operates with reduced capability.
    Degraded,
    /// The component cannot serve its contract.
    Down,
}

/// Health information for one named component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentHealth {
    /// Stable component name.
    pub name: String,
    /// Current health status.
    pub status: HealthStatus,
    /// Whether this component currently permits readiness.
    pub ready: bool,
    /// Stable, non-sensitive reason code.
    pub reason_code: Option<String>,
    /// Last update timestamp in Unix milliseconds.
    pub updated_at_ms: u128,
}

impl ComponentHealth {
    /// Builds a healthy, ready component.
    pub fn up(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Up,
            ready: true,
            reason_code: None,
            updated_at_ms: unix_millis(),
        }
    }
}

/// Aggregate liveness/readiness snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthReport {
    /// The process is alive when no component reports `Down`.
    pub live: bool,
    /// The service is ready when every component is ready and none is down.
    pub ready: bool,
    /// Deterministically ordered component snapshots.
    pub components: Vec<ComponentHealth>,
}

/// Registry updated by runtime modules and exposed by Web health adapters.
#[derive(Debug, Clone)]
pub struct HealthRegistry {
    components: Arc<RwLock<BTreeMap<String, ComponentHealth>>>,
}

impl Default for HealthRegistry {
    fn default() -> Self {
        let mut components = BTreeMap::new();
        components.insert(
            "ddd4r.runtime".to_owned(),
            ComponentHealth::up("ddd4r.runtime"),
        );
        Self {
            components: Arc::new(RwLock::new(components)),
        }
    }
}

impl HealthRegistry {
    /// Registers or replaces one component status.
    pub fn set(
        &self,
        name: impl Into<String>,
        status: HealthStatus,
        ready: bool,
        reason_code: Option<String>,
    ) -> ObservabilityResult<()> {
        let name = name.into();
        validate_component_name(&name)?;
        let component = ComponentHealth {
            name: name.clone(),
            status,
            ready,
            reason_code,
            updated_at_ms: unix_millis(),
        };
        write_unpoisoned(&self.components).insert(name, component);
        Ok(())
    }

    /// Removes a component that is no longer part of the runtime.
    pub fn remove(&self, name: &str) -> Option<ComponentHealth> {
        write_unpoisoned(&self.components).remove(name)
    }

    /// Returns a stable aggregate snapshot.
    pub fn report(&self) -> HealthReport {
        let components = read_unpoisoned(&self.components)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let live = components
            .iter()
            .all(|component| component.status != HealthStatus::Down);
        let ready = components
            .iter()
            .all(|component| component.ready && component.status != HealthStatus::Down);
        HealthReport {
            live,
            ready,
            components,
        }
    }
}

fn validate_component_name(name: &str) -> ObservabilityResult<()> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(ObservabilityError::InvalidComponentName(name.to_owned()))
    }
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_closes_when_a_required_component_is_down() {
        let registry = HealthRegistry::default();
        registry
            .set(
                "database.primary",
                HealthStatus::Down,
                false,
                Some("connection_failed".to_owned()),
            )
            .expect("component name is valid");

        let report = registry.report();
        assert!(!report.live);
        assert!(!report.ready);
        assert_eq!(report.components.len(), 2);
    }

    #[test]
    fn component_names_are_bounded_and_machine_safe() {
        let registry = HealthRegistry::default();
        assert!(
            registry
                .set("bad component", HealthStatus::Up, true, None)
                .is_err()
        );
    }
}
