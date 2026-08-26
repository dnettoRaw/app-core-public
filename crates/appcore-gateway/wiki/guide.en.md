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

> **Next-major migration:** direct access to `GatewayState::tenants` has been
> removed so unrelated tenants no longer share one lock. Use
> `tenant_partition`, `tenant_partition_or_insert`, `tenant_count` and
> `connection_count`. The former pending maps are private; use
> `pending_request_count` for observation and let `EnvelopeRouter` own their
> lifecycle. This source change must not be released as 1.0.x; the
> complete migration is in `release/gateway-tenant-migration.md`.

The gateway resolves a tenant from the deployment-owned domain suffix or an
explicit local-test query parameter, authenticates connections when configured,
routes Peer RPC envelopes and mesh-relayed Peer RPC HTTP requests only inside
the tenant partition, and tracks stale worker connections with bounded outbound
queues.

The normal Runtime activation path is the existing deployment adapter map:

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Cluster mode additionally requires absolute `paths.gateway_replay` to name one file on
a writable volume shared by every Gateway instance.

The parser accepts only those four non-secret settings. Endpoints, secret
references, unknown settings and authentication overrides fail closed.
`appcore-bin` adds and authorizes the owner descriptor `runtime.gateway` in the
shared capability catalog, reuses Runtime security and registers the instance
as a critical Supervisor-managed service. Without `adapters.gateway`, it
creates no Gateway runtime, listener or task.

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
HA, edge relay federation and alternative transports remain future work and
must not weaken Peer RPC authentication, expiry, nonce or replay protections.

The Runtime host uses a durable, process-safe `FilePeerNonceStore`: standalone
places it in private Runtime storage, while cluster fails closed unless
absolute `paths.gateway_replay` selects a shared writable file. Active sockets expire in
at most 60 seconds. Direct embedders may inject another `PeerNonceStore`; their
default is bounded and process-local. Source-IP rate limiting and TLS
termination remain deployment controls.

`GatewayRuntime` owns its listener, current-thread Tokio runtime, router,
heartbeat pruner and runtime thread. Startup binds synchronously, so an invalid
or occupied address aborts host startup. Bounded cooperative shutdown joins all
owned work. Before the deadline it force-drops the server future, closing slow
or incomplete connections before joining the thread. `Orphaned` is only a
defensive thread-failure quarantine. Safe snapshots contain lifecycle state,
bind addresses and counters only. Direct users of
`spawn_heartbeat_pruner` must retain and await the returned join handle.

Worker and client connection hashes use canonical V2 binary framing and carry
a `v2:` marker. Earlier unversioned hashes are not interchangeable; token
issuers and Gateway consumers must be upgraded together.

Each tenant keeps bounded direct worker indexes by Core ID and by
`(cluster_id, core_id)`. Routing lookup is O(1). Registration, reconnect,
disconnect and heartbeat pruning update the primary map, capability registry
and indexes under the same tenant lock. Saturating rebuild and inconsistency
counters expose index health without unbounded labels.

**Maturity:** RC peer transport profile for the V1 distributed surface.
