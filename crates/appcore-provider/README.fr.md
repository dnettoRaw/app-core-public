# appcore-provider

**Responsabilité :** factories, registry, deployment plans, contrats
coordination/job et résolution secret indépendants des implémentations.

**Dépendances internes :** `appcore-contracts`.

**API principale :** `ProviderRole`, `ProviderContext`, `ProviderFactory`,
`ProviderRegistry`, `DeploymentProviderPlan`, errors/results,
`ResolvedSecret`/`SecretProvider`; coordination schema V2, stores mémoire/file;
leases de ressource partagée avec fencing; job spec/lease/completion/provider.

Les leases filesystem utilisent un lock par ressource, un fichier d'état
versionné et un sidecar versionné de high-water epoch. Le sidecar est persisté
avant la publication du lease actif et survit au release, restart et à une
acquisition interrompue ; un epoch n'est donc jamais réutilisé. L'epoch est un fencing
token seulement pour les writers qui le vérifient avant d'écrire. Un
filesystem partagé sans lock, rename, sync de répertoire ou cohérence de cache
fiables ne donne pas une protection forte contre split brain avec cet adapter
seul.

À utiliser pour composer des providers explicites. Aucun fallback silencieux ni
SDK provider-specific dans ce crate.

**Maturité :** surface composition RC stable; jobs distribués hors du premier
profil 1.0.
