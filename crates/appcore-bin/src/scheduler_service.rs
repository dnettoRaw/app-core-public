// =============================================================================
//        #######
//     ###       ###     F: scheduler_service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime-owned lifecycle for manifest-first application tasks.

use crate::application_tasks::RegisteredApplicationTask;
use crate::bootstrap::{now_ms, BootstrapError};
use appcore_contracts::SchedulerRequirements;
use appcore_ops::{
    InMemoryObservationSink, ObservationEvent, ObservationKind, ObservationSeverity,
    ObservationSink,
};
use appcore_scheduler::{Scheduler, SchedulerConfig, SchedulerError};
use appcore_supervisor::{ManagedService, ServiceHealth};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) struct ApplicationScheduler {
    scheduler: Option<Scheduler>,
    observations: InMemoryObservationSink,
}

impl ApplicationScheduler {
    pub(crate) fn start(
        requirements: &SchedulerRequirements,
        tasks: &[RegisteredApplicationTask],
        observations: InMemoryObservationSink,
    ) -> Result<Option<Self>, BootstrapError> {
        if !requirements.is_required() {
            return Ok(None);
        }
        let max_concurrent_tasks =
            usize::try_from(requirements.max_concurrency()).map_err(|_| {
                BootstrapError::Runtime(
                    "scheduler max_concurrency is unsupported on this platform".to_string(),
                )
            })?;
        let scheduler = Scheduler::new(SchedulerConfig {
            max_tasks: tasks.len().max(1),
            max_concurrent_tasks,
            poll_interval: Duration::from_millis(25),
        })
        .map_err(scheduler_error)?;
        for registration in tasks {
            scheduler
                .schedule(
                    registration.task.clone(),
                    Arc::clone(&registration.callback),
                )
                .map_err(scheduler_error)?;
        }
        observations.emit(
            ObservationEvent::new(
                ObservationKind::Lifecycle,
                ObservationSeverity::Info,
                "runtime.scheduler.started",
                now_ms(),
            )
            .with_attribute("task_count", tasks.len().to_string())
            .with_attribute("max_concurrency", max_concurrent_tasks.to_string()),
        );
        Ok(Some(Self {
            scheduler: Some(scheduler),
            observations,
        }))
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), BootstrapError> {
        let Some(scheduler) = self.scheduler.take() else {
            return Ok(());
        };
        let result = scheduler.shutdown().map_err(scheduler_error);
        self.observations.emit(ObservationEvent::new(
            ObservationKind::Lifecycle,
            ObservationSeverity::Info,
            "runtime.scheduler.stopped",
            now_ms(),
        ));
        result
    }
}

pub(crate) fn scheduler_service_if_enabled(
    requirements: &SchedulerRequirements,
    tasks: &[RegisteredApplicationTask],
    observations: InMemoryObservationSink,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    if !requirements.is_required() {
        return Ok(None);
    }
    let requirements = requirements.clone();
    let tasks = tasks.to_vec();
    let state = Arc::new(Mutex::new(None::<ApplicationScheduler>));
    let start_state = Arc::clone(&state);
    let stop_state = Arc::clone(&state);
    let health_state = Arc::clone(&state);
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::SCHEDULER_SERVICE,
        appcore_supervisor::ManagedResource::Scheduler,
        &[],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::CallbackManagedService::new(
            descriptor,
            move || {
                let scheduler =
                    ApplicationScheduler::start(&requirements, &tasks, observations.clone())
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "scheduler requirements are disabled".to_string())?;
                let mut guard = start_state
                    .lock()
                    .map_err(|_| "scheduler state is poisoned".to_string())?;
                *guard = Some(scheduler);
                Ok(())
            },
            move |_timeout| {
                let scheduler = stop_state
                    .lock()
                    .map_err(|_| "scheduler state is poisoned".to_string())?
                    .take();
                match scheduler {
                    Some(mut scheduler) => scheduler.shutdown().map_err(|error| error.to_string()),
                    None => Ok(()),
                }
            },
            move || match health_state.lock() {
                Ok(guard) if guard.is_some() => ServiceHealth::Healthy,
                Ok(_) => ServiceHealth::Unknown,
                Err(_) => ServiceHealth::Failed,
            },
        ),
    )))
}

impl Drop for ApplicationScheduler {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn scheduler_error(error: SchedulerError) -> BootstrapError {
    BootstrapError::Runtime(format!("application scheduler failed: {error}"))
}
