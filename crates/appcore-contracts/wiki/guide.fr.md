# appcore-contracts

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** manifests et policies Runtime stables, indépendants des
implémentations.

**Dépendances internes :** aucune.

**API principale :** `ApplicationManifestV1`, `DeploymentManifestV1`,
`DeploymentManifestBuilder`, `RuntimeManifestV1`, `RuntimeMode`,
`RuntimeOperationalMode`, policies capability/storage/leadership/job/scheduler/
health/update/module, configuration provider/network/TLS/volume/environment et
`ContractError`.

À utiliser pour parser, construire et valider les contrats portables. Préserver
noms sérialisés et sens. Ne pas ajouter transport, filesystem, processus ou
métier.

**Maturité :** surface RC stable. Les changements V1 restent additifs et
compatibles sur la ligne 1.0.
