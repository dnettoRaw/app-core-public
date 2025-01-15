BEGIN;

ALTER TABLE appcore.capabilities
    ADD COLUMN IF NOT EXISTS capability_class text NOT NULL DEFAULT 'functional'
    CHECK (capability_class IN ('infrastructure', 'functional'));

CREATE INDEX IF NOT EXISTS capabilities_class_resolution_idx
    ON appcore.capabilities (tenant_id, cluster_id, capability_class, capability_id);

INSERT INTO appcore.schema_migrations (version, checksum)
VALUES (2, 'appcore-coordination-v2')
ON CONFLICT (version) DO NOTHING;

COMMIT;
