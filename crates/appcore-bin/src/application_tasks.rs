// =============================================================================
//        #######
//     ###       ###     F: application_tasks.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Application-owned task registration for the manifest-first facade.

use appcore_core::{RuntimeError, RuntimeResult};
use appcore_scheduler::{ScheduledTask, SchedulerError, TaskCallback, TaskContext, TaskResult};
use std::sync::Arc;

/// Registry of application tasks hosted by the Runtime scheduler.
#[derive(Default)]
pub struct ApplicationTaskRegistry {
    tasks: Vec<RegisteredApplicationTask>,
}

impl ApplicationTaskRegistry {
    /// Creates an empty task registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one task definition and its business callback.
    pub fn register<F>(&mut self, task: ScheduledTask, callback: F) -> RuntimeResult<()>
    where
        F: Fn(TaskContext) -> TaskResult + Send + Sync + 'static,
    {
        task.validate().map_err(invalid_task)?;
        if self
            .tasks
            .iter()
            .any(|registered| registered.task.id == task.id)
        {
            return Err(RuntimeError::RegistryItemAlreadyRegistered {
                kind: "application_task",
                name: task.id,
            });
        }
        self.tasks.push(RegisteredApplicationTask {
            task,
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

    pub(crate) fn into_tasks(self) -> Vec<RegisteredApplicationTask> {
        self.tasks
    }
}

#[derive(Clone)]
pub(crate) struct RegisteredApplicationTask {
    pub(crate) task: ScheduledTask,
    pub(crate) callback: TaskCallback,
}

fn invalid_task(error: SchedulerError) -> RuntimeError {
    RuntimeError::RegistryError(format!("invalid application task: {error}"))
}

#[cfg(test)]
#[path = "application_tasks_tests.rs"]
mod tests;
