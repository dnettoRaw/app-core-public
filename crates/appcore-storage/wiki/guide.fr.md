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

L'auth-storage distant V1 sépare ses représentations bornées : 256 Kio de
plaintext pour `seal`, 384 Kio de données scellées pour `open`, un body token
authentifié de 1 Mio et 64 Kio de headers HTTP. Un input trop grand échoue avant
l'expansion hex/JSON ; le client réutilise le buffer owned de la réponse.

L'adapter fichier scellé écrit du DNT normal par défaut et expose
`DntFileObjectStore::write_object_compact` pour snapshots, backups et fichiers
domaine exportables quand le payload est compressible. Les écritures compactes
restent des enveloppes DNT ordinaires sur le même provider fichier ; le contrat
du backend de stockage ne change pas.
Les lectures scellées dérivent une limite d'enveloppe complète depuis
`SealedStoragePolicy` et rejettent les fichiers trop grands avant l'allocation
du buffer fichier.

`FileStorageProvider::read_bytes` matérialise au maximum 64 Mio et continue la
lecture avec `max + 1` après les métadonnées, donc une croissance concurrente
ne contourne pas la limite. Le backup d'un fichier transmet au maximum 1 Gio
vers un temporaire exclusif, le synchronise puis le renomme atomiquement ; un
échec supprime le temporaire et conserve la destination précédente. Un
snapshot complet accepte au maximum 1 Gio par fichier et 16 Gio au total. Ces
limites sont exportées par `DEFAULT_FILE_READ_MAX_BYTES`,
`MAX_STORAGE_BACKUP_FILE_BYTES` et `MAX_STORAGE_SNAPSHOT_BYTES`.

Le manifest du snapshot complet est plafonné à 16 Mio. Son pretty JSON V1 est
sérialisé directement par un writer borné de 16 Kio vers un temporaire atomique
exclusif, puis désérialisé par un reader borné de 16 Kio. Le buffer encodé
complet ne coexiste plus avec l'inventaire de fichiers décodé ; une entrée
exactement à la limite reste valide et un octet non retenu détecte la
croissance.

À utiliser pour le profil local-first documenté. Garder schémas et tables
domaine hors du Runtime. Les transactions non supportées échouent.

`StorageWriteBarrier` coordonne les writers avec l'installation d'une mise à
jour : obtenez un permit `open`, appelez `block_new_writers`, puis `drain` avec
une deadline. Utilisez `release` après un drain réussi ou
`seal_after_install_start` au début de l'installation. Les permits imbriqués et
les snapshots bornés des owners sont pris en charge ; l'état scellé n'est
jamais effacé automatiquement dans le processus.

Le housekeeping et la traversée des backups sont itératifs, bornés et ne
suivent jamais les symlinks ni les reparse points Windows. Le listing utilise
les timestamps persistés dans le manifest snapshot et ne recourt aux
métadonnées de création/modification que pour les backups fichier simples.
L'ouverture finale emploie le mode no-follow de la plateforme et est revalidée
sous le lock du processus. Le profil mono-processus suppose toujours un root
protégé par son propriétaire: le remplacement hostile d'un répertoire ancêtre
par un autre processus du même compte pendant l'opération reste hors de cette
boundary portable.

La traversée visite au maximum 200 000 entrées de manière incrémentale, en ne
retenant que la pile bornée de 16 384 répertoires et les résultats requis par le
consommateur. Le snapshot conserve ses paths triés nécessaires sans seconde
liste globale ; health ne garde qu'un compteur, cleanup seulement les
temporaires correspondants et la validation des symlinks aucune entrée. La
profondeur reste plafonnée à 128.
La vérification du snapshot compte également les fichiers réels de manière
incrémentale et emprunte le path précédent pendant le contrôle de l'ordre ; elle
ne construit pas un second inventaire de paths et ne clone pas un path par
entrée.

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
