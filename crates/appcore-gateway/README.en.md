# AppCore Gateway

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

This crate implements the Gateway Capability of the AppCore Runtime.

The Gateway provides multi-tenant secure Internet access to AppCore application workers without directly exposing the workers.

> **Next-major migration:** direct access to `GatewayState::tenants` has been
> removed so unrelated tenants no longer share one lock. Embedders must use
> `tenant_partition`, `tenant_partition_or_insert`, `tenant_count` and
> `connection_count`. The former public pending maps are also private now;
> observe them with `pending_request_count` and let `EnvelopeRouter` own their
> lifecycle. See [the migration guide](../../release/gateway-tenant-migration.md).

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

`appcore-bin` is the composition root. A deployment enables this crate with
the existing adapter map:

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Cluster deployments must also point every Gateway instance at the same
process-safe replay file at an absolute path on a shared writable volume:

```toml
paths = { gateway_replay = "/shared/appcore/gateway-connection-jti.json" }
```

The adapter accepts only the four settings above. It rejects endpoints,
secret references, unknown settings and attempts to configure authentication.
Authentication always remains enabled for manifest-composed instances.

During bootstrap, the host adds the owner-defined `runtime.gateway`
capability to the catalog, authorizes it through `RuntimeCapabilityPolicy`,
reuses the Runtime security provider and registers the Gateway as a critical
Supervisor-managed service. Invalid configuration or a bind failure aborts
startup; omitting `adapters.gateway` creates no listener or task.

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
clustering, edge relays and alternative transports remain future
provider/transport work and must preserve Peer RPC authentication, expiry,
nonce and replay protection.

The Runtime host persists one-use connection identities through the
process-safe `FilePeerNonceStore`. Standalone uses private Runtime storage;
cluster mode requires an absolute `paths.gateway_replay` on one shared writable volume and
fails closed when it is absent or unavailable. Active sockets expire with their
credentials after at most 60 seconds. Direct embedders use a process-local
bounded store by default or inject a durable/shared `PeerNonceStore` through
`GatewayState::with_replay_store` or `GatewayRuntime::with_replay_store`.
Source-IP rate limiting and TLS termination remain deployment/edge controls.

`GatewayRuntime` owns the listener, runtime thread, router and heartbeat
pruner. `stop` first requests graceful shutdown, then drops the server future
before the deadline to force-close incomplete connections and joins the runtime
thread. `Orphaned` remains a defensive quarantine state for an unexpected
thread-level failure, not the normal timeout path. Its snapshot never exposes
credentials or token material. Lower-level embedders that call
`spawn_heartbeat_pruner` directly own and must await its returned join handle.

Worker and client connection hashes use canonical V2 binary framing and carry
a `v2:` marker. Earlier unversioned hashes are not interchangeable; token
issuers and Gateway consumers must be upgraded together.

Each tenant keeps bounded direct worker indexes by Core ID and by
`(cluster_id, core_id)`. Routing lookups are O(1); register, reconnect,
disconnect and heartbeat prune update the worker map, capability registry and
indexes under the same tenant lock. `worker_index_rebuilds` and
`worker_index_inconsistencies` expose bounded index-health counters.
