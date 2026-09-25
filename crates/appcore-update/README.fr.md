# appcore-update

Tests locaux:

```bash
cargo test -p appcore-update
```

**Responsabilité :** sélection, authenticité, staging, activation, health gate
et rollback d'artefact opaque.

Le contrat additionnel `ReleaseCatalog` valide des descripteurs signés et
bornés par plateforme, puis sélectionne la version compatible la plus récente
pour une `UpdateIdentity`. Les descripteurs V1 sans cible restent valides ; les
entrées du catalogue exigent un `ArtifactTarget`, dont les champs sont inclus
dans la signature. Les racines de confiance sont toujours fournies par la
politique du déploiement.

`LatestCompatibleOfferRequestV2` et `LatestCompatibleOfferResponseV2`
sélectionnent la plus grande version plus récente compatible avec la cible, la
version du Runtime et le protocole. Le pair renvoie le descripteur signé ; le
client doit appeler `verify_descriptor` avec sa politique de confiance locale
avant le staging ou la lecture des octets.

`ReleaseCatalogStore` ouvre un catalogue JSON borné sous une racine contrôlée.
Chaque entrée sépare l'emplacement relatif sûr du descripteur signé. Le store
refuse la traversée de chemin, les symlinks, les fichiers non réguliers, les
hashs dupliqués et les descripteurs absents du catalogue. Il implémente
`ArtifactSource` et ne lit que la taille déclarée de l'artefact.

`ActivationAdapter` définit la frontière hôte pour `prepare`, `activate`,
`healthcheck`, `commit`, `rollback` et `recover`. `ActivationRequest` et
`ActivationEvidence` sont bornés et peuvent devenir un `ActivationReceiptV2` ;
les installateurs desktop, raw-binary et Docker restent hors du Runtime. Les
échecs de l'adapter doivent devenir des actions de recovery explicites et ne
doivent jamais déclencher un rollback deviné.

`ArtifactSource`, `ArtifactWriter` et `receive_artifact` fournissent un chemin
de transfert synchrone et borné. Le récepteur vérifie les offsets, la taille
des blocs, la taille déclarée et le SHA-256 final, sans activer ni installer le
résultat.

`UpdateCache` ajoute un staging reprenable par hash, un verrou exclusif entre
processus, une réservation de quota, la reprise des fichiers partiels et la
publication durable du descripteur et de l'objet. Il reste séparé de
`FileArtifactStore`. Avec `secure_permissions`, le cache refuse les répertoires
et ancêtres symlinkés ou dangereusement inscriptibles, sans les réparer. La
frontière réutilisable des handles/ACL est exposée par `appcore-security` ; la
séparation des couches interdit une dépendance directe.

Les métadonnées de transfert Peer sont exposées par des contrats `offer` et
`chunk` sans chemins, pour les streams authentifiés Peer RPC V2. Le transport
reste responsable de l'authentification, de l'isolation tenant/cluster, des
deadlines, de la séquence et des hash des octets décodés ; `appcore-update`
valide l'identité de l'artefact, les offsets et les métadonnées répétées de
taille/digest.

L'API additionnelle de recovery V2 utilise `ActivationReceiptV2`,
`FileRecoveryStore`, `inspect_recovery` et des actions explicites de `replay`.
Elle conserve le receipt V1 inchangé, protège les actions par tentative et
digest, et n'effectue jamais de rollback externe implicite.

`QuarantineStore` enregistre durablement les releases en échec avec
l'application, le canal, la version, le build et le SHA-256 comme clé bornée.
Les entrées portent des motifs typés et des timestamps, survivent aux
redémarrages et aux downgrades, et restent actives jusqu'à un appel explicite à
`release` ou jusqu'à leur expiration configurée. Utilisez
`select_with_quarantine_report` pour obtenir le descripteur sélectionné et les
exclusions bornées avec clé de release, motif et expiration.

Le binaire `appcore-update-diagnose` est en lecture seule. Il inspecte un
descripteur, un répertoire de receipts, un répertoire de quarantaine ou un
cache avec une sortie humaine ou `--json`. Le JSON est versionné par
`schema_version: 1` ; les références d'artefact, signatures et host bindings
sont masqués.

L'arbre `fixtures/` et les tests d'intégration fournissent des preuves
portables pour cache partiel/corrompu, catalogues ambigus, receipts incomplets
et écritures concurrentes bornées de la quarantaine.

**Dépendances internes :** `appcore-contracts` et `appcore-provider`.

**API principale :** artifact descriptor/signing payload; verifier,
unsigned-local protégé par feature/Ed25519, trust policy/key status; update request/provider et file
factory; staged artifact, activation receipt/store; coordinator,
preparation/outcome, health check et fault injection. Ed25519 vérifie les
artefacts signés. Les artefacts locaux non signés exigent la feature explicite
`allow-unsigned-local-artifacts` et une racine locale contrôlée par le
propriétaire ; ils ne sont pas un fallback pour la supply chain distante.

À utiliser pour binaires ou artefacts opaques. Le Runtime valide identité,
version, protocole, checksum et trust sans comprendre code ou schéma.
Les migrations de schéma appartiennent à l'application.

Les lectures fichier vérifient la taille avant l'allocation, utilisent un
scratch fixe de 16 Kio plus un octet sentinelle non retenu et rejettent un
composant final non régulier. L'activation transmet le staged dans un buffer
SHA-256 fixe de 64 Kio sans matérialiser le fichier, puis crée un hard link vers
un path de build immuable. Un path existant n'est réutilisé que si taille et
digest correspondent exactement au descriptor; il n'est jamais remplacé. Le no-follow
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

## Documentation stable

Identifiant stable : **ACR-021**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-021). Cet identifiant
permanent reste valable si la page du wiki est déplacée.

`MobileUpdatePolicy` évalue une requête mobile bornée avant d’annoncer une
offre. Elle distingue remplacement autonome, store, MDM, sideload assisté par
le déploiement et cible non supportée, bloque les protocoles obsolètes et
exige une version minimale du cluster. Elle n’installe rien et ne recommande
aucun contournement de l’App Store/Play Store; ces actions restent au
déploiement.
