# appcore-storage

Tests locaux:

```bash
cargo test -p appcore-storage
```

**Responsabilité :** contrats de stockage génériques et provider fichier local
borné.

**Dépendances internes :** `appcore-contracts`, `appcore-dnt`,
`appcore-security`, `appcore-types`.

**API principale :** `StorageProvider`, `Repository`, `Migration`,
`Transaction`, health/status/errors, IDs validés, `FileStorageProvider`,
manifests storage, backup V1, helpers authentifiés de stockage distant et
stores optionnels scellés par DNT pour objets, snapshots et secrets.

L'auth-storage distant V1 accepte au maximum 256 Kio de plaintext pour `seal`,
384 Kio de données scellées pour `open`, un body token authentifié de 1 Mio et
64 Kio de headers HTTP. Les limites exportées rejettent avant l'expansion
hex/JSON ; le roundtrip maximal par défaut de 256 Kio est couvert de bout en bout.

L'adapter fichier scellé écrit du DNT normal par défaut et expose
`DntFileObjectStore::write_object_compact` pour snapshots, backups et fichiers
domaine exportables quand le payload est compressible. Les écritures compactes
restent des enveloppes DNT ordinaires sur le même provider fichier ; le contrat
du backend de stockage ne change pas.
Les lectures scellées dérivent une limite d'enveloppe complète depuis
`SealedStoragePolicy`. Les fichiers dépassant déjà cette limite au contrôle
des métadonnées sont rejetés avant l'allocation du buffer ; une croissance
pendant la lecture est bornée et rejetée après la lecture.

`FileStorageProvider::read_bytes` matérialise au maximum 64 Mio et rejette un
fichier plus grand ou qui grandit pendant la lecture. Le backup d'un fichier
est transmis vers un temporaire exclusif, accepte au maximum 1 Gio, synchronise
avant le renommage atomique et préserve le backup précédent en cas d'échec. Un
snapshot complet accepte au maximum 1 Gio par fichier et 16 Gio au total. Les
constantes exportées définissent ces limites.

Le manifest du snapshot complet est plafonné à 16 Mio. Son pretty JSON V1 est
sérialisé directement par un writer borné de 16 Kio vers un temporaire atomique
exclusif, puis désérialisé par un reader borné de 16 Kio. Le buffer encodé
complet ne coexiste plus avec l'inventaire de fichiers décodé ; une entrée
exactement à la limite reste valide et un octet non retenu détecte la
croissance.

À utiliser pour le profil local-first documenté. Garder schémas et tables
domaine hors du Runtime. Les transactions non supportées échouent.

`StorageWriteBarrier` coordonne les writers du stockage avec l'installation
d'une mise à jour. Appelez `open`, puis `block_new_writers` et `drain` avec une
deadline avant l'installation. `release` rouvre une barrière drainée non
scellée ; `seal_after_install_start` bloque les nouveaux writers jusqu'au
redémarrage. Les permits imbriqués sont pris en charge et `snapshot` expose des
owners bornés sans effacer automatiquement un owner bloqué. Il s'agit d'une
coordination d'admission, pas d'une promesse de transaction de base de données.

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

Le contrat post-1.0 `StorageCapabilityDescriptorV1` décrit transactions,
locking, snapshots, streaming, backup en ligne, multi-processus et multi-hôte
sans exposer les détails du provider. `required_capabilities` est un setting
deployment explicite; toute exigence inconnue, dupliquée ou absente échoue avant
startup. Le provider fichier annonce seulement `snapshot`. Les manifests V1 et
deployments V1 existants non partagés ne changent pas.

**Maturité :** contrats RC stables; provider fichier certifié pour un processus
local et filesystem aux sémantiques lock/sync/rename requises.

## Documentation stable

Identifiant stable : **ACR-011**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-011). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
