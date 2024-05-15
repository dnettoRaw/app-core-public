# appcore-ops

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** vendor-neutral Runtime health, logging, metrics,
observations, heartbeat and availability.

**Internal dependencies:** `appcore-core`, `appcore-supervisor`.

**Primary API:** health status/report/checks, heartbeat sources, log levels and
logger implementations, metric counters and in-memory metrics,
`ObservationEvent`/`ObservationSink`, bounded file sink and statistics,
availability reports and compatibility reexports for
`appcore-supervisor::managed_services`.

Use it for generic operational signals. New service lifecycle code uses
`appcore-supervisor` directly. Do not add vendor SDK lock-in or application
business metrics to the Runtime crate.

**Maturity:** stable RC operational primitives; production export/collection is
deployment-owned.
