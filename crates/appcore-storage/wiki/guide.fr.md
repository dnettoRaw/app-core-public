# appcore-storage

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** contrats de stockage génériques et provider fichier local
borné.

**Dépendances internes :** `appcore-contracts`, `appcore-dnt`,
`appcore-security`, `appcore-types`.

**API principale :** `StorageProvider`, `Repository`, `Migration`,
`Transaction`, health/status/errors, IDs validés, `FileStorageProvider`,
manifests storage, backup V1, helpers authentifiés de stockage distant et
stores optionnels scellés par DNT pour objets, snapshots et secrets.

L'adapter fichier scellé écrit du DNT normal par défaut et expose
`DntFileObjectStore::write_object_compact` pour snapshots, backups et fichiers
domaine exportables quand le payload est compressible. Les écritures compactes
restent des enveloppes DNT ordinaires sur le même provider fichier ; le contrat
du backend de stockage ne change pas.
Les lectures scellées dérivent une limite d'enveloppe complète depuis
`SealedStoragePolicy` et rejettent les fichiers trop grands avant l'allocation
du buffer fichier.

À utiliser pour le profil local-first documenté. Garder schémas et tables
domaine hors du Runtime. Les transactions non supportées échouent.

Le housekeeping et la traversée des backups sont itératifs, bornés et ne
suivent jamais les symlinks ni les reparse points Windows. Le listing utilise
les timestamps persistés dans le manifest snapshot et ne recourt aux
métadonnées de création/modification que pour les backups fichier simples.
L'ouverture finale emploie le mode no-follow de la plateforme et est revalidée
sous le lock du processus. Le profil mono-processus suppose toujours un root
protégé par son propriétaire: le remplacement hostile d'un répertoire ancêtre
par un autre processus du même compte pendant l'opération reste hors de cette
boundary portable.

Pour le preflight post-1.0 explicite, `StorageCapabilityDescriptorV1` utilise
sept garanties fermées et un catalogue limité à 32 providers. Le deployment
liste ses exigences exactes dans `required_capabilities`. L'exigence existante
`storage.shared=true` ajoute `multi_host`. Toute exigence inconnue, dupliquée,
indisponible ou non supportée retourne une erreur typée et redigée avant
l'ouverture; aucun fallback. Le descriptor fichier fournit seulement
`snapshot`.

[Preuve clean-source du preflight](benchmarks/storage-capability-v1-2026-08-26.fr.md)

**Maturité :** contrats RC stables; provider fichier certifié pour un processus
local et filesystem aux sémantiques lock/sync/rename requises.
