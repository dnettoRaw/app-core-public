# appcore-update

Tests locaux:

```bash
cargo test -p appcore-update
```

**Responsabilité :** sélection, authenticité, staging, activation, health gate
et rollback d'artefact opaque.

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
