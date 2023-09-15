# appcore-dnt

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** contrats et helpers du conteneur chiffré générique DNT.

**Dépendances internes :** `appcore-contracts`, `appcore-types`.

**API principale :** `seal`, `open`, `open_owned`, `inspect_header`, `verify`,
`write_atomic`, `read_verified`, `rekey`, `migrate_envelope`,
`DntKeyProvider`, `DntCodec`, `DntHeader`, `DntContext`, `DntCompression`,
`KeyId`, `ContentType`, `CodecId`, `DntFlags`, `dnt_user_flag`,
`dnt_compose_flags` et `DNT_FLAG_PAYLOAD_DEFLATE`.

DNT est une enveloppe binaire pour des octets arbitraires. `.dnt`, `.dntj`,
`.dntb` et `.dnto` sont seulement des conventions ; les consommateurs
inspectent l'en-tête authentifié.

Disposition V1 :

```text
en-tête canonique
  magic
  envelope_version
  header_length
  flags
  algorithm
  schema_version
  created_at_ms
  stored payload_length
  nonce
  payload_hash
  public_metadata_length
  encrypted_metadata_length
  application_id
  tenant_id optionnel
  content_type
  codec_id
  key_id
  public_metadata
ciphertext
  encrypted_metadata_length
  encrypted_metadata
  payload encodé stocké
tag d'authentification
```

Tout l'en-tête est l'AAD de l'AEAD. La V1 utilise XChaCha20-Poly1305 avec une
clé de 256 bits et un nonce aléatoire OS de 192 bits. Les clés sont résolues
par `DntKeyProvider` ; elles ne sont jamais stockées dans l'enveloppe.

## Pourquoi Utiliser DNT

DNT n'est pas destiné à remplacer tous les fichiers. Il est utile lorsque des
octets doivent circuler entre providers de stockage, sauvegardes, transports de
sync ou stockage local de secrets sans perdre leurs propriétés de sécurité.

Utiliser DNT lorsque le fichier exige :

- confidentialité sans placer la clé à côté des octets chiffrés ;
- identité authentifiée de l'application, du tenant, du type logique, du codec
  et de la clé ;
- rejet d'une mauvaise application, d'un mauvais tenant ou d'un mauvais type
  logique avant de retourner du plaintext ;
- détection de corruption et d'altération sur l'en-tête et le payload ;
- helpers d'écriture atomique et de lecture vérifiée ;
- rotation explicite de clé avec `rekey` ;
- migration explicite d'enveloppe avec `migrate_envelope` ;
- transport opaque par storage, sync ou gateway sans compréhension du domaine.

Ne pas utiliser DNT uniquement pour économiser du disque. JSON brut ou binaire
brut est plus simple, plus petit et plus rapide lorsqu'il n'y a pas besoin de
chiffrement, d'authentification, de liaison au contexte, de rotation de clé ou
de migration versionnée.

## Mode Compact

Le DNT normal stocke directement la sortie du codec avant chiffrement. Le DNT
compact positionne le flag authentifié `DNT_FLAG_PAYLOAD_DEFLATE` et stocke un
flux DEFLATE avec wrapper zlib à un niveau équilibré avant chiffrement. Les
lecteurs V1 inspectent les deux modes ; ouvrir une enveloppe compacte exige
`DntOpenOptions.max_payload_bytes` pour borner l'expansion.

Pour les buffers complets lus depuis un fichier, préférer `open_owned` ou
`read_verified` ; ils déchiffrent l'enveloppe propriétaire in-place. Utiliser
`open` lorsque l'appelant ne possède qu'une slice empruntée.

`read_verified` exige un `DntOpenOptions.max_payload_bytes` explicite et rejette
un fichier trop grand avant l'allocation du buffer complet. Les métadonnées
chiffrées V1 sont limitées à 64 Kio. `OpenedDnt::zeroize_plaintext` efface le
plaintext et les métadonnées chiffrées dès que l'appelant n'en a plus besoin.

| Mode | Taille disque | Chemin de lecture |
|---|---|---|
| Normal | En-tête + métadonnées chiffrées + payload encodé + tag AEAD. La taille suit la sortie du codec et le coût CPU est minimal. | Lire, authentifier, déchiffrer puis décoder le codec. C'est le chemin CPU le plus rapide pour les petits fichiers ou les données peu compressibles. |
| Compact | En-tête + métadonnées chiffrées + payload encodé compressé + tag AEAD. JSON répétitif, snapshots et logs sont souvent beaucoup plus petits ; les payloads déjà compressés ou aléatoires peuvent être identiques ou plus gros. | Lire moins d'octets disque, authentifier, déchiffrer, inflater DEFLATE puis décoder le codec. L'inflation ajoute du travail, mais moins de ciphertext peut réduire assez l'AEAD et le digest pour améliorer la latence totale des payloads très compressibles. |

Le mode compact n'est pas une frontière de sécurité. La taille du fichier révèle
encore une approximation de la taille compressée. Éviter de compacter des
secrets qui mélangent des octets contrôlés par un attaquant et des octets
confidentiels lorsque l'observation de taille compte.

### Comparaison De Référence

Le dépôt inclut une comparaison reproductible qui écrit chaque échantillon
comme fichier plaintext, DNT normal et DNT compact. Elle chauffe chaque chemin
et rapporte séparément les distributions espace, read/open, seal et rekey :

```bash
cargo run -p appcore-dnt --example compare --release
```

Exécution `--release` de référence sur Apple M1, séparée par catégorie :

Espace disque :

- JSON répétitif : plaintext 1 048 557 octets ; normal 1 048 746 ; compact 4 403 ;
- binaire incompressible : plaintext 1 048 576 octets ; normal 1 048 773 ;
  compact 1 048 949 ;
- petit secret : plaintext 65 octets ; normal 252 ; compact 254.

Médiane du chemin de lecture avec cache chaud :

- JSON répétitif : plaintext 42,7 us ; read/open normal 5,51 ms ; compact
  321,2 us ;
- binaire incompressible : plaintext 42,3 us ; normal 5,51 ms ; compact 6,33 ms ;
- petit secret : plaintext 14,5 us ; normal 17,7 us ; compact 23,8 us.

Interprétation :

- les snapshots JSON répétitifs gagnent parce que DNT authentifie et déchiffre
  beaucoup moins d'octets après compression ; lors de cette exécution,
  l'inflation de 1 Mio a coûté moins que l'AEAD et le digest du ciphertext
  supplémentaire ;
- les données binaires déterministes sont pratiquement incompressibles, donc le
  mode compact ajoute du CPU et un faible overhead de format ;
- les petits secrets se dégradent en mode compact parce que le wrapper de
  compression coûte plus d'octets et de CPU qu'il n'en économise ;
- les fichiers plaintext sont plus rapides et plus petits lorsque les
  propriétés de sécurité ne sont pas nécessaires ; cette base ne comprend pas
  chiffrement, authentification, rotation de clé, liaison au contexte ni
  détection d'altération.

Le [rapport mesuré complet](benchmarks/dnt-2026-08-02-m1.fr.md) enregistre
matériel, APFS/SSD, alimentation secteur, Rust/profile, warm-up, échantillons,
moyenne, écart, p95, p99, maximum, throughput, seal/rekey et preuves mémoire/CPU
non mesurées. Régénérer sur la classe de déploiement concernée. DNT est un
conteneur de sécurité et de portabilité, pas un remplaçant plus rapide du
plaintext de confiance.

## Flags

Le champ V1 `flags` est authentifié par l'AAD de l'en-tête AEAD et il est
partitionné pour éviter les combinaisons impossibles :

| Plage | Propriétaire | Règles |
|---|---|---|
| Bits `0..15` | comportement interne de l'enveloppe DNT/AppCore | Seuls les flags connus par ce crate sont acceptés. Les bits internes inconnus échouent avec `DntError::InvalidFlags` avant résolution de clé ou déchiffrement. |
| Bits `16..31` | annotations de l'application/appelant | DNT authentifie et préserve ces bits, mais ne leur attribue pas de sémantique centrale. Les appelants doivent les allouer avec `dnt_user_flag(index)`, où `index` vaut `0..16`. |

Utiliser `DntFlags`, `dnt_user_flag`, `dnt_compose_flags` ou
`DntSealOptions::with_user_flag` au lieu de shifts manuels. Les helpers
rejettent les index hors plage et les valeurs qui placent des flags appelant
dans la plage interne.

Modèle de menace : DNT protège la confidentialité et l'intégrité contre
l'inspection hors ligne et la modification du fichier sans la clé. Il ne
protège pas contre un processus compromis qui détient légitimement la clé en
mémoire.

**Maturité :** contrat additif post-RC. Manifest V1 ne change pas ; les
deployments sélectionnent DNT via la configuration existante de
providers/capabilities.
