# Host Update migration and recovery

Unpublished source; additive recovery API plus stricter automatic-upgrade admission. Existing published versions do not acquire these changes in place. Select a future immutable package containing them, update exhaustive `UpdateError` matches for the explicit 2.x migration, and keep registry-only consumer resolution.

## Cutover gates

1. Inventory the actual installed build/package, pending V1 activation, host journal, coordinator fence/receipt, snapshot and instance identity. Freeze new installations and drain normal writes under installation/storage locks. Resolve every ambiguous old attempt by inspecting host reality; do not reinterpret a local `prepared` phase as Runtime V2 `Prepared`.
2. Keep old executable, configuration, verified data backup and old journals. Resolve and archive the old attempt before selecting a separate V2 recovery root. Choose one authoritative lifecycle journal per layer; never dual-run old/new installers or convert journal strings automatically. Keep application schema migration outside Update.
3. Bind the Runtime attempt and `host_binding` to the host's original installation, coordinator fence, instance and snapshot. The bounded opaque binding may identify a trusted host record; do not put credentials in it. Stage only authenticated compatible artifacts, same application/channel/target, advancing version and distinct build/package digest. Explicit channel migration is a separate procedure.
4. Persist intent, drain/snapshot and seal writes before activation. After a lost activation reply, inspect installed bytes/signature and exact attempt/instance/snapshot under the same lock. Only exact active evidence may call `reconcile_activated`; ambiguity remains blocked/manual. Then run actual health and explicit commit. Query the original coordinator receipt after a lost report; never reserve a fresh attempt to hide uncertainty.
5. Retain rollback artifacts and data through health and recoverable completion. Re-probe restored identity before `RecordRolledBack`. Once new writes have been accepted, restoring an old data snapshot can lose those writes: recovery requires an application-owned compatible data plan, not automatic rollback of a previous journal.

## Separate host adapters

| Host | Required observed evidence | Migration boundary |
| --- | --- | --- |
| Desktop | Exact installed package/build, attempt, installation/snapshot, process and renderer readiness | Preserve platform handoff, candidate UI readiness, cancellation and successor identity; a process exit or HTTP health alone is insufficient |
| Raw service / systemd deployment | Verified executable/build, supervised instance, exclusive service/data ownership, heartbeat/readiness | Keep child/parent ownership, bounded stop/start and orphan drain; unit active is not exact-build health |
| Container deployment | Immutable image digest, exact container identity, volume-copy operation and candidate data volume, closed write gate | Preserve committed/released boundary and copied volumes; retry final reports without restoring old data after writes reopen |

These adapters remain deployment-owned; Runtime does not import an installer, process manager, container client or application data schema. Runtime V2 receipt, host journal and remote coordinator receipt describe different facts. `adapter.commit` runs before the Runtime terminal receipt; do not reopen normal writes merely because that callback was entered. Keep remote finalization and the irreversible write-release point explicit for each profile rather than mapping every host phase into one enum.

## Qualification and rollback

Qualify download truncation/cancellation, invalid signature, disk exhaustion, interruption during install/boot/commit and lost final responses. Assert exact identity, blocked writes on uncertainty, health before completion and data-preserving rollback. Local fake adapters qualify the contract only; native desktop, Linux service and real container runs remain separate gates.

Rollback of the library migration stops the candidate and restores a compatible prior executable/journal/data set under exclusive ownership after inspecting actual host state. Do not mix V2 evidence with a 1.x process, delete a pending receipt, reuse an attempt/fence or restart an old writer to bypass recovery. If real host/data evidence is insufficient, keep writes blocked for manual review.
