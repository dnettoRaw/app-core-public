# appcore-provider

[English](README.en.md) | [Português](README.pt.md)

`appcore-provider` définit comment un Deployment Manifest sélectionne
l’infrastructure concrète du Runtime sans faire dépendre les crates de contrat
des implémentations. Il contient les rôles, plans de construction, factories,
résolution de secrets, coordination générique, leases partagés avec fencing et
contrats de jobs.

## Flux de composition

1. Le déploiement déclare les identifiants et settings des providers.
2. La racine de composition enregistre des `ProviderFactory` explicites dans
   le `ProviderRegistry`.
3. `DeploymentProviderPlan` vérifie que chaque `ProviderRole` obligatoire
   possède exactement un provider utilisable.
4. La factory reçoit un `ProviderContext` borné et les propriétaires des
   secrets résolus, puis renvoie l’implémentation demandée.

Un provider absent ou invalide arrête le bootstrap. Le registre ne choisit
jamais de substitut silencieux. `ResolvedSecret` efface les octets qu’il possède
et ne doit pas être copié dans les diagnostics.

```rust
use appcore_provider::{
    CoordinationStoreProvider, InMemoryCoordinationStore, ProviderResult,
};

fn verifier_coordination() -> ProviderResult<u64> {
    let store = InMemoryCoordinationStore::default();
    store.ensure_compatible()?;
    store.schema_version()
}
```

## Garanties de coordination et de lease

Le store mémoire convient aux control planes embarqués ou de test. Le store
fichier utilise le schéma V2, un remplacement atomique et une limite de 4 Kio
pour les métadonnées et sources de restauration. Les lecteurs contrôlent la
taille avant allocation, gardent un octet sentinelle et refusent symlinks,
fichiers non réguliers, UTF-8 invalide et enregistrements corrompus.

Les leases fichier persistent un sidecar du plus grand epoch par ressource
avant de publier le lease actif. La libération ne réinitialise jamais la
séquence de fencing. Un token ne protège une écriture que si le writer vérifie
l’epoch courant juste avant celle-ci ; un filesystem sans locks, rename, sync
de répertoire ou cohérence de cache fiables ne protège pas fortement contre le
split-brain.

Le code propre à un provider appartient à un crate d’intégration. Consultez
l’[exemple de base](wiki/examples/basic.fr.md), l’[exemple
intermédiaire](wiki/examples/intermediate.fr.md) et le
[guide du crate](wiki/guide.fr.md).

```bash
cargo test -p appcore-provider
```

## Documentation stable

Identifiant stable : **ACR-019**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-019). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
