# appcore-contracts

[English](README.en.md) | [Português](README.pt.md)

`appcore-contracts` définit les documents versionnés échangés entre les
applications, les déploiements et les hôtes AppCore en cours d’exécution. Il
permet d’utiliser les manifestes et les politiques sans importer une
implémentation du Runtime.

## Ce qu’il contient

- `ApplicationManifestV1` : identité portable, modules, capabilities et
  exigences Runtime de l’application ;
- `DeploymentManifestV1` : mode d’installation, providers, réseau, TLS,
  volumes, environnement, références de secrets, Supervisor et watchdog ;
- `RuntimeManifestV1` : identité, mode opérationnel et santé publiés par un
  hôte actif ;
- identifiants validés et politiques de ressources, stockage, leadership,
  planification, jobs, santé et mises à jour.

Les trois manifestes ont des propriétaires distincts. L’application publie
l’Application Manifest, l’opérateur fournit le Deployment Manifest et l’hôte
produit le Runtime Manifest. Cette séparation empêche la configuration propre
à une machine de contaminer l’artefact portable de l’application.

## Quand l’utiliser

Utilisez ce crate pour construire, analyser, valider ou inspecter les contrats
AppCore V1. Les constructeurs et `validate` refusent les identifiants invalides,
les exigences absentes et les combinaisons incohérentes de politiques.

```rust
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, ContractResult,
    RuntimeRequirements, ServiceId,
};

fn manifeste() -> ContractResult<ApplicationManifestV1> {
    ApplicationManifestV1::new(
        ApplicationId::new("notes-app")?,
        "1.0.0",
        "Notes",
        "example-vendor",
        ServiceId::new("notes")?,
        RuntimeRequirements::new("1.0.0", "1")?,
    )
}
```

## Limites

Ce crate de contrats ne contient ni I/O, ni implémentation de provider, ni
listener, ni cycle de vie de processus, ni schéma métier. Les noms sérialisés
V1 forment une barrière de compatibilité : les ajouts compatibles peuvent
évoluer en V1, mais les entrées supprimées ou incompatibles ne sont ni devinées
ni converties silencieusement.

Consultez l’[exemple de base](wiki/examples/basic.fr.md), l’[exemple
intermédiaire](wiki/examples/intermediate.fr.md) et le
[guide du crate](wiki/guide.fr.md). Exécutez les tests ciblés avec :

```bash
cargo test -p appcore-contracts
```

## Documentation stable

Identifiant stable : **ACR-002**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-002). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
