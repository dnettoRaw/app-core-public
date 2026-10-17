# appcore-contracts

Release candidate **`appcore-contracts 1.0.4-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.

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
