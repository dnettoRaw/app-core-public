# appcore-types

[English](README.en.md) | [Português](README.pt.md)

`appcore-types` fournit le vocabulaire validé partagé par les contrats et les
composants Runtime AppCore. Il remplace les chaînes non contrôlées aux
frontières de processus, de stockage et de protocole par de petits types qui
appliquent partout les mêmes règles.

## Contrats principaux

- identifiants d’application, tenant, cluster, nœud, Core, instance,
  capability, command, query, event, state et groupe de synchronisation ;
- `RuntimeIdentity` et `CoreIdentity`, avec politique et état de compatibilité ;
- `CapabilityDescriptor`, manifestes de Core distribué et endpoints de peers ;
- `TraceContext` pour corréler trace, span, parent, command, origine et tenant ;
- `RuntimeError` et `RuntimeResult` pour les défaillances fondamentales
  contrôlées.

Créez les identifiants à la première frontière non fiable, puis transmettez la
valeur typée. Une trace enfant conserve trace, tenant, origine et command tout
en recevant un nouveau span et le Core courant.

```rust
use appcore_types::{CoreId, RuntimeResult, TenantId, TraceContext};

fn trace_enfant() -> RuntimeResult<TraceContext> {
    let api = CoreId::new("core-api")?;
    let racine = TraceContext::new(
        "trace-42",
        "span-api",
        api.clone(),
        api,
        TenantId::new("tenant-a")?,
    )?;

    racine.child_span("span-worker", CoreId::new("core-worker")?)
}
```

## Limites et erreurs

Ce crate ne contient ni I/O, ni état Runtime mutable, ni comportement de
provider, ni modèle métier. Les longueurs, caractères, noms réservés et
identités incohérentes sont refusés dès la construction, avant d’atteindre un
autre sous-système. Les versions de protocole et de contrat restent des valeurs
explicites ; elles ne sont pas déduites d’un peer réseau.

Consultez l’[exemple de base](wiki/examples/basic.fr.md), l’[exemple
intermédiaire](wiki/examples/intermediate.fr.md) et le
[guide du crate](wiki/guide.fr.md). Exécutez :

```bash
cargo test -p appcore-types
```

## Documentation stable

Identifiant stable : **ACR-003**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-003). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
