# appcore-update

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** sélection, authenticité, staging, activation, health gate
et rollback d'artefact opaque.

**Dépendances internes :** contracts et provider.

**API principale :** artifact descriptor/signing payload; verifier,
unsigned-local protégé par feature/Ed25519, trust policy/key status; update request/provider et file
factory; staged artifact, activation receipt/store; coordinator,
preparation/outcome, health check et fault injection.

À utiliser pour binaires ou artefacts opaques. Le Runtime valide identité,
version, protocole, checksum et trust sans comprendre code ou schéma.

`ReleaseCatalog` est un contrat additionnel pour des cibles de plateforme
signées. Utilisez `ArtifactTarget` pour le système, l'architecture et le format
de l'hôte ; la cible est couverte par la signature du descripteur. Le catalogue
valide toutes les entrées avant sélection, refuse les clés dupliquées
identité/canal/cible/version et ne fournit jamais ses propres racines de
confiance. Les descripteurs V1 restent utilisables hors catalogue.

Pour une sélection initiée par un pair, utilisez
`LatestCompatibleOfferRequestV2` et `LatestCompatibleOfferResponseV2`. La
sélection exclut les versions égales ou antérieures et vérifie la cible, le
canal, la version du Runtime et le protocole. Le pair ne remplace pas la
politique de confiance locale : appelez `verify_descriptor` avec
`ArtifactAuthenticityVerifier` avant d'accepter le descripteur.

Utilisez `ReleaseCatalogStore` lorsque les octets locaux sont publiés avec un
catalogue. Les entrées séparent l'emplacement relatif sûr du descripteur signé
et les chunks ne sont servis que si le descripteur complet appartient au
catalogue validé. La racine contrôlée refuse la traversée, les symlinks, les
fichiers non réguliers et les hashs dupliqués ambigus.

L'activation hôte est exprimée par `ActivationAdapter` : implémentez
`prepare`, `activate`, `healthcheck`, `commit`, `rollback` et `recover` pour le
déploiement choisi. `ActivationRequest` et `ActivationEvidence` rejoignent la
frontière du receipt V2 ; les installateurs de plateforme restent hors d'AppCore
et les actions de recovery sont explicites.

Pour un transfert borné, implémentez `ArtifactSource` et `ArtifactWriter`,
puis utilisez `receive_artifact`. Le récepteur vérifie l'offset demandé, la
limite du bloc, la taille déclarée et le SHA-256 final. La réception reste
séparée du staging, de l'activation et de l'installation.

Utilisez `UpdateCache` pour le staging reprenable des téléchargements. Il gère
les fichiers `.part`, les objets adressés par hash, les métadonnées du
descripteur, le verrou entre processus et le quota. Il n'active pas les
artefacts et ne supprime pas les releases protégées ; `FileArtifactStore` reste
le store d'installation et de rollback. Avec `secure_permissions`, il refuse
les répertoires et ancêtres symlinkés ou dangereusement inscriptibles sans les
réparer. La frontière réutilisable des handles/ACL reste dans
`appcore-security`, tandis que la séparation des couches maintient ce crate
indépendant.

Pour Peer RPC V2, transportez `ArtifactOfferRequestV2` et
`ArtifactChunkRequestV2` comme payloads de requête bornés, puis répétez les
métadonnées dans `ArtifactOfferResponseV2` ou
`ArtifactChunkResponseV2`. Ces payloads ne contiennent aucun chemin distant.
Utilisez le stream V2 existant pour les octets ; l'autorisation et le framing du
transport restent hors de ce crate.

Pour le recovery, créez un `ActivationReceiptV2` dans `FileRecoveryStore`,
appelez `inspect_recovery` au démarrage et n'appliquez qu'une
`RecoveryAction` explicite. `replay` protège l'action par `attempt_id` et
digest. L'hôte doit exécuter et observer tout rollback externe avant de
l'enregistrer ; la sémantique V1 reste inchangée.

Utilisez `QuarantineStore` après un échec du healthcheck ou de l'activation. Sa
clé bornée inclut l'application, le canal, la version, le build et le digest.
La quarantaine est durable, survit aux downgrades et redémarrages, et ne peut
être libérée que par l'opération explicite `release`. `quarantine_until` prend
en charge une expiration exclusive ; `select_with_quarantine_report` explique
les exclusions actives avec leur motif, build et clé SHA-256. Les entrées
expirées restent dans les diagnostics mais ne bloquent plus la sélection.

Exécutez `appcore-update-diagnose --json descriptor <fichier>`,
`receipt <répertoire>`, `quarantine <répertoire>` ou `cache <répertoire>` pour
une inspection sûre en CI. L'outil n'active, ne répare, ne libère et ne
supprime jamais rien. Les classes de sortie sont stables : `64` usage, `65`
données invalides, `66` entrée absente et `74` I/O.

Les fixtures couvrent les caches partiels et corrompus, la sélection de
catalogue ambiguë et les receipts de recovery incomplets. Le harness exerce
aussi le verrou de processus avec des écritures concurrentes bornées de la
quarantaine ; la durabilité filesystem native reste une preuve spécifique à
chaque plateforme.

Les lectures fichier vérifient la taille avant l'allocation, utilisent un
scratch fixe de 16 Kio plus un octet sentinelle non retenu et rejettent un
composant final non régulier. L'activation transmet la validation de taille et
SHA-256 dans un buffer fixe de 64 Kio, puis crée un hard link vers un path de
build immuable. Un path existant n'est réutilisé que si taille et digest
correspondent exactement au descriptor; il n'est jamais remplacé. Le no-follow
atomique du composant final existe sous Unix. Les autres plateformes conservent
les checks metadata mais dépendent de la frontière filesystem du déploiement
contre les races de reparse.

Les pointers active/previous et les receipts d'activation pending empruntent
leurs descriptors, passent un sizing sans rétention sous 1 Mio et sérialisent
directement dans le temporaire atomique avec un buffer fixe de 16 Kio. Leur
lecture désérialise aussi directement par un reader borné fixe de 16 Kio, sans
conserver un vecteur complet d'octets encodés avec le pointer ou receipt décodé.
L'absence, l'échec d'I/O et l'échec de décodage restent distincts afin de
préserver l'upgrade wall de l'activation pending. Le JSON V1 reste inchangé.

Le file provider parcourt une seule fois l'index borné en streaming et ne
retient que la meilleure version sémantique et son descriptor. Chaque
descriptor est validé puis éliminé ou sélectionné pendant le décodage du tableau
JSON, sans vecteur de descriptors ni liste triée de candidats. À version égale,
la première entrée reste prioritaire. Un reader fixe de 16 Kio, un preflight de
1 Mio et un octet sentinelle non retenu rejettent une taille déclarée excessive
et une croissance concurrente.

**Maturité :** lifecycle RC stable; supply chain distant exige signature,
provenance et trust roots.

Pour mobile, construisez `MobileUpdatePolicy` avec l’action du déploiement, la
version minimale du cluster et le protocole requis. Évaluez version installée,
candidat, cluster et target avant d’annoncer la disponibilité. Un protocole
incompatible bloque le client; un cluster ancien bloque l’offre. Le contrat
décrit seulement la politique et n’exécute aucune action store, MDM ou sideload.
