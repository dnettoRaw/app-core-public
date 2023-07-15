# Validar e deduplicar transporte opaco

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Um gateway pode validar metadados de roteamento sem abrir conteudo criptografado
da aplicacao. Aplique o deduplicador limitado somente apos a politica aceitar o
envelope.

```rust
use appcore_distributed_contracts::{
    OpaqueContentEnvelopeV1, OpaqueEnvelopeDecision,
    OpaqueEnvelopeDeduplicator, OpaqueEnvelopePolicy,
};
use appcore_types::{CapabilityName, InstanceId, RuntimeResult, TenantId};

fn main() -> RuntimeResult<()> {
    let capability = CapabilityName::new("sync.consume")?;
    let policy = OpaqueEnvelopePolicy {
        accepted_envelope_versions: vec![1],
        max_payload_bytes: 64 * 1024,
        accepted_capabilities: vec![capability.clone()],
    };
    let envelope = OpaqueContentEnvelopeV1::new(
        TenantId::new("tenant-acme")?,
        InstanceId::new("notes-eu-1")?,
        "message-00042",
        Some("sync-cycle-7".into()),
        capability,
        vec![0x44, 0x4e, 0x54, 0x01],
        1,
        1_700_000_000_000,
        1_700_000_060_000,
        Some(5),
    );

    if envelope.validate_transport(&policy, 1_700_000_010_000)
        != OpaqueEnvelopeDecision::Accepted
    {
        return Err(appcore_types::RuntimeError::InvalidRequest {
            kind: "opaque envelope",
            reason: "transport policy rejected",
        });
    }

    let mut deduplicator = OpaqueEnvelopeDeduplicator::new(10_000);
    assert_eq!(
        deduplicator.accept(&envelope.message_id),
        OpaqueEnvelopeDecision::Accepted
    );
    assert_eq!(
        deduplicator.accept(&envelope.message_id),
        OpaqueEnvelopeDecision::Duplicate
    );
    Ok(())
}
```

O transporte enxerga apenas metadados limitados e bytes opacos. Descriptografia
e validacao do schema da aplicacao permanecem no consumer de destino.
