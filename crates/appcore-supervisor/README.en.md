# appcore-supervisor

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

`appcore-supervisor` owns dependency-aware lifecycle management for Runtime
services. It provides `ManagedService`, qualified dependencies, activation and
runtime states, `RestartPolicy`, bounded budgets and queue, a fixed restart
executor, lifecycle events, shutdown deadlines, `SupervisorWatchdog`,
diagnostics, and adapters for threads and embedded resources.

The crate has independent SemVer and no AppCore dependencies. It can supervise
managed services in any Rust process; AppCore is one consumer.

It supervises Scheduler, Peer RPC, Control Plane, Jobs, Update, Auth Server,
Metrics, Observation, Sync, and other infrastructure services through generic
contracts. It never supervises or replaces the operating-system process. A
service manager such as systemd, launchd, Windows Service Control Manager, or a
container orchestrator remains responsible for the process.

Reconcile never sleeps or performs restart inline. Services with an
insufficient dependency become `Degraded` without spending restart budget. An
orphaned thread is quarantined and cannot be replaced in-process. Exhausting
the restart budget also requires operator action.

Adapter callbacks and restart workers convert panics into failed lifecycle
outcomes, and deadline arithmetic is checked. Shutdown remains cooperative: an
arbitrary callback that ignores cancellation cannot be forcibly stopped safely
inside the process.

`appcore-ops::service_supervisor` contains deprecated aliases only. New code
imports this crate directly.
