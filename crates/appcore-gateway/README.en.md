# AppCore Gateway

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

This crate implements the Gateway Capability of the AppCore Runtime.

**Responsibility:** tenant-isolated WebSocket relay between external clients
and AppCore workers.

**Internal dependencies:** contracts, types, security, distributed contracts
and Peer RPC.

**Main API:** `GatewayConfig`, `GatewayState`, tenant state, capability registry
and resolver, bounded worker/client connections, `MeshPeerTransport`, mesh
relay request/response DTOs, heartbeat pruning and the Axum router factory.
Opaque content-envelope contracts are reexported for encrypted payload routing.

The Gateway provides multi-tenant secure Internet access to AppCore application workers without directly exposing the workers.

> **Current RC migration:** direct access to `GatewayState::tenants` has been
> removed so unrelated tenants no longer share one lock. Code using that field
> intentionally fails to compile and must use
> `tenant_partition`, `tenant_partition_or_insert`, `tenant_count` and
> `connection_count`. The former public pending maps are also private now;
> observe them with `pending_request_count` and let `EnvelopeRouter` own their
> lifecycle. No compatibility alias or mirror map is provided. See
> [the migration guide](../../release/gateway-tenant-migration.md).

The private directory keeps 32 immutable copy-on-write shard generations.
Whole-directory admission, heartbeat and HA scans clone only 32 `Arc` handles,
release every shard lock, and then inspect the shared tenant partitions. They
do not allocate or clone a complete tenant list.

## Architecture

```
Browser / Client
      │
HTTPS / WebSocket (JSON PeerRpcEnvelope / PeerRpcResponse)
      │
*.<deployment-domain>
      │
AppCore Gateway
      │
WebSocket / RPC / mesh-relay
      │
Workers
```

The gateway domain is deployment-specific. Each deployer configures its own
`domain_suffix` through `GatewayConfig::new(bind_address, "gateway.example.com")`.
An incoming request to `tenant-a.gateway.example.com` resolves to
`TenantId("tenant-a")`.

## Runtime composition

Gateway composition is deployment-owned. A deployment enables this crate with
the existing adapter map:

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

`GET /v1/gateway/diagnostics/peers` provides an authenticated, payload-free
snapshot of connected peers and advertised capabilities. It accepts tenant,
cluster and capability filters and reports bounded identity, heartbeat,
health, inflight and worker-count metadata. Tokens, payloads and socket
credentials are never returned.

## Key Responsibilities

1. **Connection Management**: Multiplexes and holds WebSocket connections from workers and clients.
2. **Authentication**: Validates workers and clients cryptographically using `appcore-security` and `appcore-types` (Tenant boundaries).
3. **Multi-Tenant Routing**: Partitions all lookups and connections strictly by `TenantId`. Connections never cross tenant boundaries.
4. **Presence and Heartbeats**: Tracks active workers and capability registrations. Prunes stale nodes.
5. **Mesh Relay**: Carries logical Peer RPC HTTP requests over outbound-only worker connections.
6. **Bounded Backpressure**: Uses fixed outbound frame queues per connection.
7. **No Business Logic**: Only acts as a secure envelope relay, knowing nothing of business schemas, databases, or application logic.

## Connection authentication

Authenticated upgrades accept credentials only through the `Authorization:
Bearer ...` header. Query-string credentials are rejected. A worker supplies
`cluster`, `installation`, `core` and bounded `capabilities`; a client supplies
`cluster` and `device`. Use `worker_connection_hash` or
`client_connection_hash` as the `request_hash` of a short-lived `peer` token
issued with a unique `jti`. The token is single-use and may live for at most 60
seconds. The resulting socket expires with the token.

The mesh relay parses only the Peer RPC transport envelope. It checks request
ID, tenant, target Core, cluster, capability, payload digest and signed request
hash before selecting a worker. It never interprets the opaque application
payload.

Frames and messages are limited to 4 MiB. Tenant, connection, capability,
pending-request, timeout, queue and concurrent-routing limits fail closed.
Heartbeat text must exactly match the versioned heartbeat JSON shape.

## Scope

`mesh-relay` is a peer transport profile for Cores that can make outbound
Gateway connections but cannot expose stable ports or IPs. This crate is not a
consensus system, TLS terminator or production secret manager. Gateway
HA has an opt-in provider contract described below. Further edge relays and
alternative transports must preserve Peer RPC authentication, expiry,
nonce and replay protection; the existing HA code is not a consensus system.

The current RC exposes the provider-independent HA ownership contract:
`GatewayRegistryProvider`, tenant-local `GatewayInstanceLease`, fenced
worker/session records and `GatewayRequestFence`. Records are bounded,
versioned and redact federation URLs plus request/session identities from
`Debug`. The default build keeps this contract and all single-instance Gateway
behavior without linking a Redis client. Enable the additive `ha-redis` Cargo
feature only in the integration that composes the Redis HA provider:

```toml
appcore-gateway = { version = "2.0.0-alpha.2", features = ["ha-redis"] }
```

`RedisGatewayRegistryProvider` implements the contract with an
explicit TLS/loopback endpoint policy, a separately resolved zeroizing
credential, bounded commands and tenant-local atomic scripts. It never retries
an ambiguous mutation; callers must enter isolation and call `reconnect`
explicitly. `GatewayHaCoordinator` now acquires and renews the complete,
bounded tenant lease set before moving its lifecycle to `Healthy`; a partial,
stale or uncertain round clears local leases and enters `Isolated`. Each round
is serialized, has at most 64 concurrent operations and a five-second total
deadline. Renewal shares immutable lease and worker snapshots instead of
copying every owned identity before each provider round; live mutations use
copy-on-write under the serialized coordinator boundary. The opt-in
`GatewayHaLifecycle` fails HTTP/WebSocket admission,
dispatch and response completion closed outside `Healthy`, while unconfigured
single-instance state is unchanged. `GatewayRuntime::with_ha_coordinator` owns
the recovery/shutdown task, replays the complete bounded worker/session
snapshot before `Healthy`, registers new sockets before local admission and
removes exact records on disconnect or heartbeat pruning. Shared request
fencing now claims origin/target epochs and worker generation before local
dispatch, completes before returning success and cancels on queue failure,
timeout or shutdown; an aborted future expires within 30 seconds. The provider
can check the exact live claim without consuming it before target admission. The V2
federation schema now binds source/target epochs, worker generation and inner
request to a separate one-use credential and returns typed AC-021 errors. Its
bounded HTTP route now passes a two-Gateway-state end-to-end test and completes
the shared fence before accepting a response. The same test passes with Redis
7.4 and through Caddy 2.11.4 without direct-origin bypass. Owner-loss recovery
also routes again under a higher epoch after the bounded lease TTL. AC-022 and
platform certification are still required before this becomes a deployable
two-instance HA profile, and it must not use local fallback. See
[`release/gateway-ha-redis-v2.md`](../../release/gateway-ha-redis-v2.md).

Direct embedders use a bounded process-local replay store by default. For
restart-safe or shared connection-token replay protection, the deployment must
inject an appropriate `PeerNonceStore` through `GatewayState::with_replay_store`
or `GatewayRuntime::with_replay_store`. `FilePeerNonceStore` is process-safe
under its filesystem contract; a local file alone is not cross-host coordination.
The removed Runtime host no longer wires a store or `paths.gateway_replay`.
Live sockets expire with their credentials within 60 seconds. HA leases and
request fencing do not replace connection-token replay admission. Source-IP
rate limiting and TLS termination remain deployment controls.

`GatewayRuntime` owns the listener, runtime thread, router and heartbeat
pruner. `stop` first requests graceful shutdown, then drops the server future
before the deadline to force-close incomplete connections and joins the runtime
thread. `Orphaned` remains a defensive quarantine state for an unexpected
thread-level failure, not the normal timeout path. Its snapshot never exposes
credentials or token material. Lower-level embedders that call
`spawn_heartbeat_pruner` directly own and must await its returned join handle.

The runtime and federation transport use at most 16 blocking threads and 16
pre-admission permits, with 1 MiB stacks and five-second idle retirement. A
saturated federation gate rejects and cancels its request fence before queueing.
Each admitted federation request is moved into that blocking worker, and its
encoded JSON buffer is then moved into the HTTP request. The inner Peer RPC
body therefore has no two additional complete payload-sized copies in flight.
The outer credential hash uses `json_payload_hash` to serialize canonical JSON
directly into SHA-256 before the one required wire buffer is created; it does
not retain another hash-only `Vec`.

Worker and client connection hashes use canonical V2 binary framing and carry
a `v2:` marker. Earlier unversioned hashes are not interchangeable; token
issuers and Gateway consumers must be upgraded together.
The hash borrows its validation fields. Maximum worker capability framing
writes directly into its final 17 KiB hexadecimal output, without the former
8.5 KiB binary frame or a second 17 KiB hash-only string. Capability parsing
retains one owned validated name and a borrowed deduplication entry per unique
capability.

Each tenant keeps bounded direct worker indexes by Core ID and by
`(cluster_id, core_id)`. The common unique-Core lookup is O(1); duplicate Core
IDs use a scan bounded by the tenant worker ceiling. Register, reconnect,
disconnect and heartbeat prune update the worker map, capability registry and
indexes under the same tenant lock. `worker_index_rebuilds` and
`worker_index_inconsistencies` expose bounded index-health counters.

The capability registry retains one shared name owner per distinct tenant-local
capability instead of one string allocation per worker advertisement. The
direct capability-to-worker map remains the routing source of truth.
`capabilities_for_iter` reads one worker's stable ordered names without cloning,
while `stats` reports only distinct names, workers, advertisements and unique
UTF-8 name bytes. Deregistration releases a name when its final advertiser
leaves; registry clones share immutable names but keep independent indexes.

## Deterministic worker selection in `1.0.3-rc`

The exhaustive V1 `SelectionPolicy` remains limited to `FirstAvailable`.
`WorkerSelectionPolicy` provides opt-in `RoundRobin`, `LeastInflight`,
`HealthWeighted` and `Affinity` choices while `FirstAvailable` remains the
default. RC consumers using the advanced variants must update the enum name;
no manifest or wire contract changes. Candidate identity order is stable rather than dependent on
`HashSet` iteration. `CapabilityResolver::select` accepts bounded live inputs
and rejects absent capabilities, stale/disconnected workers, exhausted
workers, and invalid affinity with distinct `WorkerSelectionError` values.
Selection borrows worker identities: first-available, least-inflight and
affinity keep no candidate list, while round-robin and health-weighted use one
compact borrowed buffer to preserve stable order. Only the selected key is
cloned for the owned public result.
Candidate lookup no longer clones installation and Core IDs to construct a
temporary tuple key. It uses the existing Core index in the common case and an
exact bounded scan when multiple installations share a Core ID. In matched
complete Gateway certification runs, allocation operations fell 89.10%,
requested bytes fell 45.21%, and selection p99 improved by 15.63% to 20.87%.

Affinity uses no retained map: rendezvous hashing includes the tenant,
capability, bounded key and worker identity. Actual Peer RPC and mesh dispatch
does not rewrite the signed V1 target. It independently enforces at most 64
inflight routes per worker with a permit released on every terminal path.
Planning therefore cannot bypass admission, and telemetry exposes fixed
unhealthy/capacity outcomes plus the worker inflight peak without identity
labels. See [`release/gateway-worker-selection-rc.md`](../../release/gateway-worker-selection-rc.md).

## Bounded telemetry in `1.0.4-rc`

`GatewayMetrics::telemetry_snapshot` and `GatewayRuntime::details`
expose fixed-bucket p50/p95/p99 route, worker-wait, tenant-lock and payload
measurements. They also expose inflight/peak, queue-depth peak, reconnect,
retry, authentication, saturation, timeout, unhealthy/capacity rejection,
worker-inflight peak, overflow and exporter-failure counters. At most 128
validated capability names are retained; later names use one fixed overflow
series. Tenant, installation, Core, request, connection, credential, payload
and error text are never labels.

`GatewayTelemetryExporter` receives only an owned snapshot when an operator
calls `export_telemetry`; routing never invokes exporters or vendor SDKs.
Prometheus/OpenTelemetry adapters remain deployment-owned and must bound their
own queues. Stable 1.0 counters remain unchanged; the detailed contract is a
RC addition.

**Maturity:** RC peer-transport profile for the distributed V1 surface.

## Stable documentation

Stable ID: **ACR-018**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-018). This permanent ID
remains valid if the wiki page moves.
