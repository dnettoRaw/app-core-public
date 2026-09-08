# appcore-supervisor

Concurrent or reentrant start/stop calls fail immediately without executing a
second lifecycle callback. Health observation does not hold the lifecycle gate.

`CallbackManagedService` treats a stop error or panic as incomplete resource
teardown: its state becomes permanently `Orphaned`, and subsequent start/stop
calls fail closed. Supervisor records quarantine and operator intervention.
A scheduler stop callback must return an error when `shutdown_with_timeout`
returns `Ok(false)`; never translate that result into success. Callbacks still
must honor their time budget; the adapter cannot preempt a blocked callback.

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** dependency-aware lifecycle, health, restart budgets and
shutdown for Runtime-owned managed services.

**Internal dependencies:** none.

**Versioning:** independent SemVer. The crate can be consumed without any
other AppCore package.

**Primary API:** `ManagedService`, `ServiceDescriptor`, `ServiceDependency`,
`DependencyRequirement`, `Supervisor`, `SupervisorWatchdog`, `RestartPolicy`,
`RestartState`, `ServiceHealth`, `ServiceActivationState`,
`ServiceRuntimeState`, typed snapshots/events, and adapters.

Use it in a composition root to manage Scheduler, Peer RPC, Control Plane,
Jobs, Update, Auth Server, Metrics, Observation, Sync, workers and queues. Do
not use it to restart its own host process. Reconcile only schedules restart
work. A bounded executor performs lifecycle actions while an independent
watchdog verifies progress.

No second Supervisor module or alias surface exists in `appcore-ops`.

Managed callback, factory and health-probe panics become controlled failed
states; a panic in one restart job does not terminate the bounded worker.
Timeout arithmetic and pending counters are checked. Restart commands and
completions use separate bounded queues. A stalled reconcile applies
cancellable completion backpressure, while shutdown closes admission and
drains retained entries. Shutdown remains cooperative, so an arbitrary callback
that ignores cancellation cannot be forcibly stopped safely in-process.

**Maturity:** evolving stable contract with bounded events, queue, workers,
budgets and diagnostics; deployment process supervision remains external.
