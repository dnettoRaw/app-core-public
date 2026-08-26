# appcore-core

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** generic in-process Runtime lifecycle, registration,
dispatch, state, audit and idempotency.

**Internal dependencies:** `appcore-contracts`, `appcore-types`.

**Primary API:** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, command/event registries and buses, envelopes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log and journal,
in-memory/file idempotency, state and decision registries/engines, clock,
redaction and compatibility `AppPlugin`.

Cloned `RuntimeController` values share lifecycle, idempotency and in-flight
command state. The immutable command bus owns handlers through `Arc`.
Independent handlers may execute concurrently, while one idempotency key admits
at most one execution. Request shutdown before calling the bounded in-flight
drain; new commands are then rejected without racing the lifecycle transition.

New applications consume these re-exports through
`appcore_bin::application`; they do not assemble the core manually. Keep I/O
adapters and domain behavior outside this crate.

**Maturity:** stable low-level RC surface; builder/plugin APIs are compatibility
level, while manifest-first hosting is preferred.
