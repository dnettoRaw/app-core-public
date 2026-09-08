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

Tests locaux:

```bash
cargo test -p appcore-core
```

Les registries et engines de décision admettent au plus 4 096 noms uniques
de 1–256 octets UTF-8. Une inscription invalide, dupliquée ou excessive échoue
avant rétention et préserve l'ordre existant. Cela borne les métadonnées du
registry, pas la mémoire interne des nœuds fournis par l'application.

**Responsabilité :** lifecycle, enregistrement, dispatch, state, audit et
idempotence génériques dans le processus.

**Dépendances internes :** `appcore-contracts`, `appcore-types`.

**API principale :** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries et buses command/event, enveloppes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotence mémoire/fichier, state et decision engines, clock, redaction et
`AppPlugin` de compatibilité.

Les clones de `RuntimeController` partagent lifecycle, idempotence et commandes
en cours. Le command bus immuable possède les handlers via `Arc`. Les handlers
indépendants peuvent s'exécuter en parallèle, tandis qu'une même clé idempotente
n'admet qu'une exécution. Le shutdown ferme l'admission atomiquement et permet
un drainage borné des commandes admises.

`RuntimeLifecycle` conserve un seul enum d'état `Copy` sous son mutex et
applique les 12 transitions stables exactes par une fonction totale. Aucun nom
validé ni table de transitions n'est alloué par instance. La `StateMachine`
publique générique reste disponible et inchangée pour les états applicatifs.

L'idempotence fichier est parcourue progressivement. Le journal V1 limite le
fichier à 64 Mio, chaque enregistrement à 1 Mio, l'intervalle entre compactages
à 131 072 enregistrements persistés et l'état actif à 65 536 clés. L'ajout
compacte atomiquement avant de franchir une limite ; le démarrage ignore
uniquement un dernier enregistrement incomplet et borné, sans accepter les
enregistrements complets corrompus. Le store ne conserve que les clés et
offsets vérifiés et charge un corps de réponse à la demande au lieu de garder
tous les replays dans le heap. L'idempotence mémoire utilise la même limite de
clés actives.

`FileOperationalJournal` parcourt aussi une ligne bornée à la fois et rejette
un enregistrement supérieur à 1 Mio. Le hash, l'ajout et le compactage atomique
sérialisent directement vers les compteurs, digests et fichiers, sans buffer
JSON complet ni clone des enregistrements retenus. `write_audit_jsonl` transmet
l'export vers un writer du caller ; `export_audit_jsonl` reste l'adaptateur
compatible retournant une `String` owned.

L'`AuditLog` local au processus a un budget agrégé par défaut de 16 Mio pour
ses snapshots de commandes et d'entrées génériques, en plus des plafonds de
10 000 éléments. `with_max_bytes` peut le réduire, `stats` expose les octets
courants/de pic, évictions et rejets, et `write_jsonl` sérialise un snapshot
copy-on-write partagé vers le writer du caller sans garder le lock pendant les
I/O. Cloner le log partage les snapshots immuables jusqu'à une mutation.

Utilisez `entries_snapshot` lorsqu'un export structuré et borné exige un tableau
JSON. Il capture la file immuable partagée sans cloner les champs des entrées et
implémente `Serialize` ; les mutations ultérieures ne modifient pas cette vue.
Le benchmark pretty-JSON de 10 000 entrées et 2 996 676 octets a mesuré 1,12 ms
p50 et 6,42 Mio de RSS de pic sur Apple M1.

`records_snapshot` fournit le même contrat pour les enregistrements command.
Les deux types de snapshot exposent `recent(limit)` afin qu'une query bornée
emprunte seulement sa page la plus récente après libération du lock. Une tail de
1 000 éléments sur 10 000 enregistrements et entrées a mesuré 2,06 us p50 et
11,88 Mio de RSS de pic, contre 4,16 ms et 20,33 Mio pour les copies complètes.

Avec un `FileOperationalJournal` attaché, les nouvelles entrées d'audit et les
entrées restaurées sûres partagent avec le journal une seule allocation
immuable de l'enregistrement opérationnel. Le chargement du journal valide la
chaîne de hash, vérifie le texte audit sans allocation, assainit uniquement le
contenu à risque et le réécrit atomiquement avant de l'exposer. L'attachement
ultérieur du log ne copie donc que des handles `Arc` bornés. Les accesseurs
owned publics, le JSON du snapshot et le format V1 ne changent pas. Un
attachement de 384 entrées sûres (environ 3 Mio) a réduit le p50 de 12,26 ms à
86,50 us (-99,29 %), le RSS de pic de 0,57 % et le RSS de la charge de 1,72 %
sur Apple M1. La charge fsync séparée conserve ses gains antérieurs de 27,83 %
en p50, 37,30 % en RSS de pic et 47,93 % en mémoire retenue.

L'`EventBus` local au processus retient lui aussi au plus 10 000 événements et
16 Mio par défaut. `with_max_bytes` réduit ce plafond, `stats` expose la
pression en octets, les évictions et rejets d'événements trop grands, et
`snapshot().recent` emprunte une page récente stable. Sélectionner 1 000 sur
10 000 événements a mesuré 2,39 us p50 et 8,48 Mio de RSS de pic, contre
2,09 ms et 14,59 Mio pour `events()`.
Lorsqu'un `FileOperationalJournal` est attaché, le bus et le journal conservent
la même allocation immuable de l'enregistrement événement. La restauration ne
copie que des handles `Arc` bornés ; les API publiques owned et le format V1 du
journal restent inchangés. Une charge de 3 Mio d'événements retenus a réduit le
RSS de pic de 8,11 à 5,08 Mio (-37,38 %) et la mémoire retenue de 48,00 %, avec
un p50 dominé par le disque dans une variation de 0,95 %.

Les nouvelles applications utilisent `appcore_sdk::Application`; elles
n'assemblent pas le core. Garder I/O adapters et comportement domaine hors de
ce crate.

**Maturité :** surface low-level RC stable; builder/plugin restent de
compatibilité, manifest-first est préféré.

## Documentation stable

Identifiant stable : **ACR-008**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-008). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
