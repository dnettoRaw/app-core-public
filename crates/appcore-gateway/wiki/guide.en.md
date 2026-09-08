# appcore-gateway

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** tenant-isolated WebSocket relay for Gateway connections
between external clients and AppCore workers.

**Internal dependencies:** contracts, types, security, distributed
contracts and peer RPC.

**Primary API:** `GatewayConfig`, `GatewayState`, tenant state, capability
registry and resolver, bounded worker/client connection handles,
`MeshPeerTransport`, mesh relay request/response DTOs, heartbeat pruner and
Axum router factory. Opaque content-envelope transport contracts are reexported
for encrypted payload routing.

> **Current RC migration:** direct access to `GatewayState::tenants` has been
> removed so unrelated tenants no longer share one lock. Code using that field
> intentionally fails to compile; use
> `tenant_partition`, `tenant_partition_or_insert`, `tenant_count` and
> `connection_count`. The former pending maps are private; use
> `pending_request_count` for observation and let `EnvelopeRouter` own their
> lifecycle. No compatibility alias or mirror map is provided. The complete migration
> is in `release/gateway-tenant-migration.md`.

The private directory stores 32 immutable copy-on-write shard generations.
Admission, heartbeat and HA scans clone only those 32 `Arc` handles and release
all shard locks before inspecting shared tenant partitions; no complete tenant
list is allocated or cloned.

The gateway resolves a tenant from the deployment-owned domain suffix or an
explicit local-test query parameter, authenticates connections when configured,
routes Peer RPC envelopes and mesh-relayed Peer RPC HTTP requests only inside
the tenant partition, and tracks stale worker connections with bounded outbound
queues.

An explicit deployment can declare Gateway configuration in its adapter map:

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

The manifest declares configuration; it does not start the Gateway.
Deployment integration passes the selected provider to
`GatewayConfig::from_provider_config`, which accepts only the four settings
above and rejects endpoints, secret references, unknown settings and attempts
to disable authentication.

The deployment owns capability authorization, the security provider, explicit
replay-store injection, Supervisor registration, startup and shutdown.
`GatewayRuntime::new` creates a stopped runtime with bounded process-local
replay protection. For restart-safe/shared replay, explicitly use
`GatewayRuntime::with_replay_store`; HA composition uses
`GatewayRuntime::with_ha_coordinator` with both the store and coordinator.
A manifest path alone does not construct either object. A shared file is
suitable only when its filesystem satisfies the store's process-safety
contract; a local file does not coordinate distinct hosts.

The SDK does not perform this wiring. Invalid configuration and startup/bind
failures must be propagated by the deployment; configuration alone creates
no listener or task.

Authenticated upgrades accept credentials only in the `Authorization` header;
query credentials are rejected. Worker tokens use `worker_connection_hash` to
bind tenant, cluster, installation, Core and capabilities. Client tokens use
`client_connection_hash` to bind tenant, cluster and device. Both are one-use
`peer` tokens with a unique `jti`, a request hash and at most 60 seconds of
lifetime; the socket expires with the token.

The mesh relay validates its V1 schema and the inner Peer RPC routing metadata,
body digest and signed request hash before forwarding. Application payloads
remain opaque. Frames/messages are limited to 4 MiB; tenant, connection,
capability, pending-request, timeout, queue and concurrent-routing limits fail
closed. Heartbeats require the exact JSON heartbeat shape, and a worker response
is accepted only from the selected connection generation.

`mesh-relay` is a peer transport for Cores that keep outbound-only Gateway
connections instead of exposing local ports or stable IPs. It is not a
consensus system, public TLS terminator or production secret manager. Gateway
HA has an opt-in provider contract described below. Further edge relays and
alternative transports must not weaken Peer RPC authentication, expiry,
nonce or replay protections.

Direct embedders use a bounded process-local replay store by default. For
restart-safe or shared connection-token replay protection, the deployment must
inject an appropriate `PeerNonceStore` through `GatewayState::with_replay_store`
or `GatewayRuntime::with_replay_store`. `FilePeerNonceStore` is process-safe
under its filesystem contract; a local file alone is not cross-host coordination.
The removed Runtime host no longer wires a store or `paths.gateway_replay`.
Live sockets expire with their credentials within 60 seconds. HA leases and
request fencing do not replace connection-token replay admission. Source-IP
rate limiting and TLS termination remain deployment controls.

`GatewayRuntime` owns its listener, current-thread Tokio runtime, router,
heartbeat pruner and runtime thread. Startup binds synchronously, so an invalid
or occupied address aborts host startup. Bounded cooperative shutdown joins all
owned work. Before the deadline it force-drops the server future, closing slow
or incomplete connections before joining the thread. `Orphaned` is only a
defensive thread-failure quarantine. Safe snapshots contain lifecycle state,
bind addresses and counters only. Direct users of
`spawn_heartbeat_pruner` must retain and await the returned join handle.

The runtime and federation transport use at most 16 blocking threads and 16
pre-admission permits, with 1 MiB stacks and five-second idle retirement. A
full federation gate cancels its request fence before rejecting the route.
An admitted federation request transfers ownership to the blocking worker and
then transfers its encoded JSON buffer to `HttpRequest`; it does not clone the
complete inner Peer RPC payload at either boundary.
Its outer credential uses `json_payload_hash` to stream canonical JSON into
SHA-256, so hashing retains no second complete encoded body.

Worker and client connection hashes use canonical V2 binary framing and carry
a `v2:` marker. Earlier unversioned hashes are not interchangeable; token
issuers and Gateway consumers must be upgraded together.
Hashing borrows every validation field. At the 64 x 128-byte capability bound,
framing writes directly into the final 17 KiB hexadecimal output instead of
retaining an 8.5 KiB binary frame and a second 17 KiB hash-only string. The
parser owns each unique validated name once and deduplicates with borrowed
slices. See the [connection-hash benchmark](benchmarks/gateway-connection-hash-2026-09-03.en.md).

Each tenant keeps bounded direct worker indexes by Core ID and by
`(cluster_id, core_id)`. The common unique-Core lookup is O(1); duplicate Core
IDs use a scan bounded by the tenant worker ceiling. Registration, reconnect,
disconnect and heartbeat pruning update the primary map, capability registry
and indexes under the same tenant lock. Saturating rebuild and inconsistency
counters expose index health without unbounded labels.

## HA registry ownership (`1.0.2-rc` contract)

`GatewayRegistryProvider` defines asynchronous tenant-local instance leases,
worker and session ownership, bounded resolution and in-flight request
claim/completion. `GatewayInstanceLease` carries a monotonic epoch;
`GatewayWorkerRecord` also binds the local connection generation; and
`GatewayRequestFence` binds origin epoch, target epoch and worker generation.
Every mutation must atomically compare these values.

`GatewayFederationUrl` accepts HTTPS or loopback-only HTTP, rejects embedded
credentials and redacts its value from `Debug`. Request and session records
also omit their identities from debug output.

The default crate build includes the provider-independent HA contract but no
Redis client, TLS stack, or Redis credential owner. The composition crate that
selects this integration must opt in explicitly:

```toml
appcore-gateway = { version = "1.0.6-rc", features = ["ha-redis"] }
```

`RedisGatewayRegistryProvider` now implements this contract. Configure it with
`RedisGatewayRegistryConfig`, convert the deployment `ResolvedSecret` with
`RedisGatewayCredential::new(secret.into_zeroizing())`, and pass that owner to
`connect`; credential values are not accepted in the endpoint. Plain Redis is
loopback-only, while non-loopback endpoints require `rediss://`. Operation
timeouts are at most 5 seconds, concurrency at most 64, instance/worker leases
at most 60 seconds and resolution at most 1,024 workers. Tenant scripts enforce
1,024 workers, 4,096 sessions and 2,048 pending requests.

Transport uncertainty returns `Unavailable` without retrying an ambiguous
mutation. The lifecycle owner must enter isolation and invoke `reconnect`
explicitly before reacquiring a higher epoch. `GatewayHaLifecycle` exposes the
fixed `Stopped`, `Recovering`, `Healthy` and `Isolated` modes plus bounded
transition/recovery/fencing counters. Attaching it through
`GatewayState::with_ha_lifecycle` makes HTTP/WebSocket admission, request
dispatch and response completion fail closed outside `Healthy`. State created
without it preserves single-instance behavior.

`GatewayHaCoordinator` owns a fixed, unique and bounded list of tenant/cluster
bindings for one instance. It acquires every tenant epoch before `Healthy`,
renews the entire exact set, rolls back completed acquisitions after a partial
failure and clears all local leases on an uncertain or stale renewal. Rounds
are serialized, use at most 64 provider operations concurrently and have a
five-second total deadline. Renewal shares private immutable lease and worker
snapshots; it does not copy complete ownership lists before provider I/O, and
serialized live mutations use copy-on-write. Its cooperative loop retries
recovery while isolated and releases exact leases after stopping admission.

`GatewayRuntime::with_ha_coordinator` owns that loop and supplies its local
snapshot. Recovery re-registers every bounded live worker and unexpired session
before `Healthy`. A new socket is shared before local admission; disconnect,
heartbeat pruning and shutdown remove the exact record. Snapshot telemetry
exposes only fixed lifecycle and ownership counts.

The local route now claims origin/target epochs and worker generation before
dispatch, completes the fence before returning success and cancels it after a
queue failure, timeout or shutdown. A route future aborted by its owner leaves
only a provider record bounded by the 30-second request TTL. A target can check
the exact live claim without consuming it before admission. Fixed coordinator
counters expose claims, completions and cancellations without request labels.
The strict V2 federation schema binds that fence and the inner request to a
separate one-use credential and returns typed AC-021 errors. Its bounded HTTP
route passes a two-Gateway-state E2E and completes the fence before accepting a
response. The combined deployment proof uses Redis 7.4 and Caddy 2.11.4 without
direct-origin bypass, drops the owner ungracefully and routes through Caddy
again with a higher epoch after the bounded lease TTL. Platform certification
remains pending.

The AC-022 local release harness also measures shared lookup and complete
recovery at 1, 100 and 1,000 tenants, then 64 successful routes through each
local and federated path. It uses an in-process provider to isolate contract
overhead; the combined Redis, proxy and owner-loss evidence remains a separate
ignored deployment test. Platform CI evidence is still required before calling the two-instance profile
deployable. The local directory never becomes fallback truth.

## Worker selection (`1.0.3-rc`)

`FirstAvailable` remains the compatible default and now uses stable identity
order. Opt-in `RoundRobin`, `LeastInflight`, `HealthWeighted` and `Affinity`
policies operate only on the current tenant's capability registry. Use the
live selector before constructing and signing the explicit Peer RPC target:

```rust
use appcore_gateway::{
    CapabilityResolver, WorkerSelectionInput, WorkerSelectionPolicy,
};
use std::time::Duration;

tenant.resolver = CapabilityResolver::with_policy(WorkerSelectionPolicy::LeastInflight);
let selected = tenant.select_worker(
    &capability,
    WorkerSelectionInput::new(now_ms, Duration::from_secs(90)),
)?;
```

The exhaustive V1 `SelectionPolicy` remains limited to `FirstAvailable`.
Advanced policies use the new non-exhaustive `WorkerSelectionPolicy`; this
preserves source compatibility for stable V1 consumers.

All live policies reject closed/stale workers, full outbound queues and
workers at their bounded inflight limit. Health weighting uses fixed
1-through-16 heartbeat-freshness weights. Affinity accepts at most 128 bytes
and uses stateless tenant-local rendezvous hashing, so it retains no key map.
Actual dispatch independently acquires a 64-route worker permit and releases
it on success, failure, timeout, cancellation and shutdown. The Gateway never
rewrites the signed V1 target or silently falls back to another policy.
First-available, least-inflight and affinity scan borrowed worker identities
without a candidate allocation. Round-robin and health-weighted use one compact
borrowed buffer because their stable ordered distribution requires it; only the
selected public result clones its key.
Worker lookup itself uses the existing Core index without allocating temporary
installation/Core tuple keys. If installations share one Core ID, an exact scan
bounded by the 1,024-worker tenant limit preserves identity. Matched complete
Gateway certification runs reduced allocation count 89.10%, requested bytes
45.21%, and selection p99 by 15.63% to 20.87%.
The clean reference measurements are recorded in the
[Gateway worker-selection benchmark](benchmarks/gateway-worker-selection-2026-08-26.en.md).

The reverse registry interns each distinct capability name across all worker
advertisements in the tenant while preserving the direct routing index.
`CapabilityRegistry::capabilities_for_iter` exposes stable borrowed names and
`CapabilityRegistry::stats` exposes payload-free ownership counts. Removing the
last advertiser releases the shared name. The bounded A/B evidence is in the
[capability-registry benchmark](benchmarks/gateway-capability-registry-2026-09-03.en.md).

## Bounded capability telemetry (`1.0.4-rc`)

Every route updates one fixed outcome and fixed histograms for complete latency,
worker wait, tenant-lock wait and opaque payload bytes. The process snapshot
also reports inflight/peak, queue-depth peak, reconnects, explicit retries,
authentication failures, unhealthy/capacity rejections, worker-inflight peak,
overflow and exporter failures. Percentiles are bucket upper bounds rather
than retained samples.

The registry retains 128 validated capability labels plus one fixed overflow
series. It never labels by tenant, installation, Core, request, connection,
token, payload or dynamic error. `GatewayTelemetryExporter` is a pull boundary:
callers explicitly pass it an immutable snapshot outside routing locks. A
failed exporter increments `export_failures` and returns only to that caller;
it cannot reject or delay a route because routing never invokes it.

The release gate exercises 4,096 instrumented rejected routes and 256 snapshots
at maximum cardinality. Budgets are 1 ms p99 per route and 5 ms p99 per
snapshot. Prometheus and OpenTelemetry adapters consume this same bounded
contract outside the crate and own their queues, retry and transport policy.
The clean reference measurements are recorded in the
[Gateway telemetry benchmark](benchmarks/gateway-telemetry-2026-08-26.en.md).

**Maturity:** current RC peer transport profile for V1; detailed telemetry is an RC
alpha contract.
