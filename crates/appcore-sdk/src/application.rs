// =============================================================================
//        #######
//     ###       ###     F: application.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Contracts used by business code hosted by `AppCore`.
//!
//! Applications declare behavior here. Manifest I/O, provider resolution,
//! service wiring, concurrency and shutdown belong to the chosen deployment
//! process and subsystem crates. Keeping this boundary in the `SDK` lets an
//! application evolve without importing a composition crate as a library.

#[cfg(feature = "deployment")]
use crate::context::DeploymentContext;
#[cfg(feature = "api")]
pub use appcore_api::{
    ApiMethod, ApiRequest, ApiResponse, ApiRouter, CommandRequest, QueryEndpoint, QueryName,
    QueryRequest, QueryResponse,
};
pub use appcore_core::{
    AppFamily, AppId, CommandBus, CommandEnvelope, CommandHandler, CommandName, CommandRegistry,
    CommandResult, DecisionEngine, DecisionNode, DecisionOutcome, DecisionRegistry, EventEnvelope,
    EventName, EventRegistry, NodeId, RuntimeContext, RuntimeContractVersion, RuntimeError,
    RuntimeIdentity, RuntimeResult, StateName, StateRegistry, SyncGroup,
};
#[cfg(feature = "scheduler")]
pub use appcore_scheduler::{RetryPolicy, ScheduledTask, TaskContext, TaskResult, TaskSchedule};
#[cfg(feature = "scheduler")]
use appcore_scheduler::{SchedulerError, TaskCallback};
#[cfg(feature = "scheduler")]
use std::sync::Arc;

/// Business behavior registered with an `AppCore` deployment.
///
/// Implementations contain application-owned behavior only. Infrastructure
/// ownership stays with the selected host and subsystem crates.
pub trait Application: Send + Sync {
    /// Applies validated installation bindings before behavior registration.
    #[cfg(feature = "deployment")]
    fn configure(&self, _deployment: &DeploymentContext) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers command names exposed by the application.
    fn register_commands(&self, _registry: &mut CommandRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers event names emitted by the application.
    fn register_events(&self, _registry: &mut EventRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers application state contracts.
    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers decision names for introspection.
    fn register_decisions(&self, _registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers executable decision nodes.
    fn register_decision_nodes(&self, _engine: &mut DecisionEngine) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers executable command handlers.
    fn register_handlers(&self, _bus: &mut CommandBus) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers side-effect-free application query endpoints.
    #[cfg(feature = "api")]
    fn register_queries(&self, _router: &mut ApiRouter) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers bounded background tasks declared by the application.
    #[cfg(feature = "scheduler")]
    fn register_tasks(&self, _registry: &mut ApplicationTaskRegistry) -> RuntimeResult<()> {
        Ok(())
    }
}

/// Application-owned scheduled tasks collected before a deployment starts workers.
#[cfg(feature = "scheduler")]
#[derive(Default)]
pub struct ApplicationTaskRegistry {
    tasks: Vec<RegisteredApplicationTask>,
}

#[cfg(feature = "scheduler")]
impl ApplicationTaskRegistry {
    /// Creates an empty task registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one validated task definition and its callback.
    pub fn register<F>(&mut self, task: ScheduledTask, callback: F) -> RuntimeResult<()>
    where
        F: Fn(TaskContext) -> TaskResult + Send + Sync + 'static,
    {
        task.validate().map_err(invalid_task)?;
        if self
            .tasks
            .iter()
            .any(|registered| registered.definition.id == task.id)
        {
            return Err(RuntimeError::RegistryItemAlreadyRegistered {
                kind: "application_task",
                name: task.id,
            });
        }
        self.tasks.push(RegisteredApplicationTask {
            definition: task,
            callback: Arc::new(callback),
        });
        Ok(())
    }

    /// Returns the number of registered tasks.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Reports whether no tasks are registered.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Transfers registrations to the deployment integration.
    #[doc(hidden)]
    pub fn into_registered(self) -> Vec<RegisteredApplicationTask> {
        self.tasks
    }
}

/// One validated task registration passed from application code to a deployment.
///
/// This is public only so deployment integration can consume registrations; it
/// is not part of the normal application authoring surface.
#[cfg(feature = "scheduler")]
#[doc(hidden)]
#[derive(Clone)]
pub struct RegisteredApplicationTask {
    definition: ScheduledTask,
    callback: TaskCallback,
}

#[cfg(feature = "scheduler")]
impl RegisteredApplicationTask {
    /// Returns the task definition selected by application business code.
    #[doc(hidden)]
    pub fn definition(&self) -> &ScheduledTask {
        &self.definition
    }

    /// Returns the callback owned by the application registration.
    #[doc(hidden)]
    pub fn callback(&self) -> &TaskCallback {
        &self.callback
    }
}

#[cfg(feature = "scheduler")]
fn invalid_task(error: SchedulerError) -> RuntimeError {
    RuntimeError::RegistryError(format!("invalid application task: {error}"))
}

#[cfg(all(test, feature = "scheduler"))]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn rejects_duplicate_task_identity() {
        let task = || ScheduledTask {
            id: "sdk.task".to_string(),
            schedule: TaskSchedule::Once {
                run_at: SystemTime::now(),
            },
            retry: RetryPolicy::default(),
            priority: 0,
            trace: None,
        };
        let mut registry = ApplicationTaskRegistry::new();
        registry.register(task(), |_| Ok(())).expect("first task");
        assert!(registry.register(task(), |_| Ok(())).is_err());
    }
}
