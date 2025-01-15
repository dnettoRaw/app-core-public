BEGIN;

CREATE SCHEMA IF NOT EXISTS appcore;

CREATE TABLE IF NOT EXISTS appcore.schema_migrations (
    version bigint PRIMARY KEY,
    applied_at timestamptz NOT NULL DEFAULT now(),
    checksum text NOT NULL
);

CREATE TABLE IF NOT EXISTS appcore.tenants (
    tenant_id text PRIMARY KEY,
    enabled boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS appcore.runtime_instances (
    tenant_id text NOT NULL REFERENCES appcore.tenants (tenant_id),
    cluster_id text NOT NULL,
    instance_id text NOT NULL,
    node_id text NOT NULL,
    core_id text NOT NULL,
    service_id text NOT NULL,
    runtime_version text NOT NULL,
    protocol_version text NOT NULL,
    runtime_mode text NOT NULL CHECK (runtime_mode IN ('standalone', 'cluster')),
    operational_mode text NOT NULL,
    health_status text NOT NULL CHECK (health_status IN ('healthy', 'degraded', 'unhealthy')),
    last_seen_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    runtime_manifest jsonb NOT NULL,
    PRIMARY KEY (tenant_id, cluster_id, instance_id),
    UNIQUE (tenant_id, cluster_id, core_id)
);

CREATE INDEX IF NOT EXISTS runtime_instances_discovery_idx
    ON appcore.runtime_instances (tenant_id, cluster_id, service_id, health_status, expires_at);

CREATE TABLE IF NOT EXISTS appcore.capabilities (
    tenant_id text NOT NULL,
    cluster_id text NOT NULL,
    instance_id text NOT NULL,
    capability_id text NOT NULL,
    capability_version text NOT NULL,
    mode text NOT NULL CHECK (mode IN ('query', 'command', 'stream')),
    visibility text NOT NULL CHECK (visibility IN ('local', 'cluster', 'tenant')),
    requires_leader boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, cluster_id, instance_id, capability_id),
    FOREIGN KEY (tenant_id, cluster_id, instance_id)
        REFERENCES appcore.runtime_instances (tenant_id, cluster_id, instance_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS capabilities_resolution_idx
    ON appcore.capabilities (tenant_id, cluster_id, capability_id);

CREATE TABLE IF NOT EXISTS appcore.leases (
    tenant_id text NOT NULL,
    cluster_id text NOT NULL,
    service_id text NOT NULL,
    holder_core_id text NOT NULL,
    epoch bigint NOT NULL CHECK (epoch > 0),
    acquired_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, cluster_id, service_id),
    CHECK (expires_at > acquired_at)
);

CREATE INDEX IF NOT EXISTS leases_expiration_idx
    ON appcore.leases (expires_at);

CREATE TABLE IF NOT EXISTS appcore.jobs (
    tenant_id text NOT NULL REFERENCES appcore.tenants (tenant_id),
    cluster_id text NOT NULL,
    job_id text NOT NULL,
    capability_id text NOT NULL,
    state text NOT NULL CHECK (state IN ('pending', 'claimed', 'completed', 'failed', 'cancelled')),
    payload_reference text,
    claimed_by_core_id text,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    available_at timestamptz NOT NULL,
    lease_expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, cluster_id, job_id)
);

CREATE INDEX IF NOT EXISTS jobs_claim_idx
    ON appcore.jobs (tenant_id, cluster_id, capability_id, state, available_at);

CREATE TABLE IF NOT EXISTS appcore.audit (
    audit_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id text NOT NULL REFERENCES appcore.tenants (tenant_id),
    cluster_id text,
    core_id text,
    service_id text,
    action text NOT NULL,
    outcome text NOT NULL,
    trace_id text,
    occurred_at timestamptz NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS audit_scope_time_idx
    ON appcore.audit (tenant_id, cluster_id, occurred_at DESC);

CREATE TABLE IF NOT EXISTS appcore.runtime_versions (
    runtime_version text NOT NULL,
    protocol_version text NOT NULL,
    build_id text NOT NULL,
    update_channel text NOT NULL,
    artifact_reference text NOT NULL,
    checksum text NOT NULL,
    published_at timestamptz NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (runtime_version, build_id)
);

INSERT INTO appcore.schema_migrations (version, checksum)
VALUES (1, 'appcore-coordination-v1')
ON CONFLICT (version) DO NOTHING;

COMMIT;
