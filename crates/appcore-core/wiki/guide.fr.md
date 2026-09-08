# appcore-core

Le benchmark runtime compare la redaction actuelle à l'algorithme préservé de
`efe9a205` : `redaction_plain_8192_{current,reference}` et
`redaction_mixed_{current,reference}`. Création des fixtures et égalité des sorties
sont hors chronométrage ; chaque itération inclut redaction et libération du
résultat. Les implémentations utilisent le même binaire optimisé, avec des
échantillons en processus séparés. Ces cas mesurent le texte diagnostique, pas
l'I/O du journal entier ni le nombre d'allocations. La référence est réservée au benchmark.

La redaction vérifie si le texte entier est déjà sûr et borné avant les passages
par marqueurs ; ce chemin alloue seulement le résultat owned. Sinon, les passages
gardent la recherche précédente sur copie en minuscules, mais réutilisent la
sortie lorsqu'un marqueur est absent. Les variantes de recherche directe ont
été rejetées après des régressions mesurées sur texte mixte.
Les sept passages préservent ordre, délimiteurs et troncature
UTF-8 antérieurs. Le résultat public reste une String owned ; les passages avec
matches peuvent allouer. Aucun gain de débit ou RSS n'est affirmé sans benchmark
apparié. Cela reste une redaction conservatrice par marqueurs, pas un parser
capable d'identifier chaque secret dans des payloads arbitraires.

Les registries et engines de décision admettent au plus 4 096 noms uniques
de 1–256 octets UTF-8. Une inscription invalide, dupliquée ou excessive échoue
avant rétention et préserve l'ordre existant. Cela borne les métadonnées du
registry, pas la mémoire interne des nœuds fournis par l'application.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** lifecycle, enregistrement, dispatch, state, audit et
idempotence génériques dans le processus.

**Dépendances internes :** `appcore-contracts`, `appcore-types`.

**API principale :** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries et buses command/event, enveloppes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotence mémoire/fichier, state et decision engines, clock, redaction et
`AppPlugin` de compatibilité.

Les valeurs clonées de `RuntimeController` partagent lifecycle, idempotence et
commandes en cours. Le command bus immuable possède les handlers via `Arc`. Les
handlers indépendants peuvent s'exécuter en parallèle, tandis qu'une clé
idempotente n'admet qu'une exécution. Demandez le shutdown avant le drainage
borné ; les nouvelles commandes sont alors rejetées sans course avec la
transition lifecycle.

`RuntimeLifecycle` conserve un seul enum d'état `Copy` sous son mutex et
applique les 12 transitions stables exactes par une fonction totale. Aucun nom
validé ni table de transitions n'est alloué par instance. La `StateMachine`
publique générique reste disponible et inchangée pour les états applicatifs.

`FileIdempotencyStore` parcourt son journal V1 une ligne bornée à la fois sans
matérialiser le fichier. Les limites du fichier, d'un enregistrement et des clés
actives sont 64 Mio, 1 Mio et 65 536 ; au plus 131 072 enregistrements de
journal sont conservés entre compactages atomiques. Une dernière ligne
incomplète et bornée est récupérée, tandis qu'une ligne complète invalide
échoue fermée. La map résidente contient des clés et offsets vérifiés par
SHA-256, pas les corps de réponse ; `get` ne lit que l'enregistrement borné
sélectionné. `InMemoryIdempotencyStore` partage la limite de clés actives.

`FileOperationalJournal` applique la même discipline incrémentale à la
persistance audit et événements. Au démarrage, au plus une ligne de 1 Mio reste
en mémoire pendant la validation de la hash chain. L'ajout calcule le hash via
un compteur borné et un digest writer, puis le compactage trouve le plus grand
suffixe qui tient avant de transmettre un seul remplacement atomique. Utilisez
`write_audit_jsonl` avec un sink possédé par le caller ; la méthode existante
`export_audit_jsonl` matérialise intentionnellement la `String` demandée.

L'`AuditLog` en mémoire borne séparément ses deux snapshots à 10 000 éléments
et utilise un budget partagé par défaut de 16 Mio. Utilisez `with_max_bytes`
pour le réduire, consultez `stats` pour la pression courante/de pic, les
évictions et rejets, et préférez `write_jsonl` pour transmettre un snapshot
copy-on-write après libération du lock d'état. `export_jsonl` reste
l'adaptateur owned compatible.

Pour un tableau JSON structuré, appelez `entries_snapshot`. La vue immuable
retournée implémente `Serialize`, partage les entrées au lieu de les cloner en
profondeur et reste stable si le log actif change. La charge pretty-JSON de
10 000 entrées et 2 996 676 octets a mesuré 1,12 ms p50 et 6,42 Mio de RSS de
pic sur Apple M1.

`records_snapshot` est la vue correspondante des enregistrements command. Les
deux snapshots offrent `recent(limit)` pour une page récente empruntée après
libération du lock. Sélectionner 1 000 éléments sur 10 000 enregistrements et
entrées a mesuré 2,06 us p50 et 11,88 Mio de RSS de pic, contre 4,16 ms et
20,33 Mio pour les copies owned complètes.

Lorsque `AuditLog` est attaché à `FileOperationalJournal`, les nouvelles
entrées et les entrées restaurées sûres conservent le même
`Arc<OperationalJournalRecord>` immuable. Le chargement du journal valide
d'abord la chaîne de hash, puis applique une vérification de texte sans
allocation. Le contenu à risque est borné, expurgé et réécrit atomiquement une
seule fois ; les attachements suivants ne copient que des handles `Arc` bornés.
Les accesseurs owned publics, la sérialisation du snapshot et l'encodage V1
restent inchangés. Un attachement de 384 entrées sûres (environ 3 Mio) a réduit
le p50 de 12,26 ms à 86,50 us (-99,29 %), le RSS de pic de 0,57 % et le RSS de
la charge de 1,72 % sur Apple M1. La charge fsync appariée conserve ses gains
antérieurs de 27,83 % en p50, 37,30 % en RSS de pic et 47,93 % en mémoire
retenue.

L'`EventBus` local au processus possède la même forme mémoire explicite : au
plus 10 000 événements et un budget partagé par défaut de 16 Mio. Utilisez
`with_max_bytes` pour le réduire ; `stats` expose les octets courants/de pic,
évictions et rejets ; `snapshot().recent(limit)` sélectionne une page stable
sans cloner les payloads. La sélection de 1 000 sur 10 000 a mesuré 2,39 us p50
et 8,48 Mio de RSS de pic, contre 2,09 ms et 14,59 Mio pour l'adaptateur de
copie complète.
Avec un `FileOperationalJournal` attaché, les deux owners conservent un seul
`Arc<OperationalJournalRecord>` immuable par événement au lieu de deux
allocations du payload. La restauration ne copie que des handles `Arc` bornés.
Les accesseurs owned publics, la sérialisation du snapshot et le format V1 sur
disque restent inchangés. Une charge de 3 Mio a réduit le RSS de pic de 8,11 à
5,08 Mio (-37,38 %) et la mémoire retenue de 48,00 % ; le p50 dominé par le
disque a varié de +0,95 %.

Les nouvelles applications utilisent `appcore_sdk::Application`; elles
n'assemblent pas le core. Garder I/O adapters et comportement domaine hors de
ce crate.

**Maturité :** surface low-level RC stable; builder/plugin restent de
compatibilité, manifest-first est préféré.
