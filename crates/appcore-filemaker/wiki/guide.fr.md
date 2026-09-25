# Guide appcore-filemaker

Les messages diagnostiques sont coupés uniquement aux frontières UTF-8 : les
erreurs conservent au plus 1 024 octets ; paths source, messages de validation
et pertes d'export au plus 512. Les grands buffers sont remplacés par le préfixe
borné. Cela borne le texte retenu, pas l'allocation préalable de l'appelant,
l'overhead de l'allocateur ni le nombre de rapports accumulés.

Exécutez `cargo test -p appcore-filemaker --test reflow` pour vérifier la limite
exacte de tentatives push, le rejet d'un gap négatif et l'arrêt shrink à la taille
minimale. Une erreur de limite ne prouve pas l'exécution du détecteur de cycle.

`reflow_dense_64` résout 64 rectangles initialement superposés avec une limite
de 64 tentatives et vérifie chaque position finale. `reflow_limit_63` exige
l'erreur explicite de limite avec 63 tentatives sur la même fixture. Compilation
et binding restent hors mesure ; création du moteur, layout/reflow, vérifications
et libération sont chronométrés. Ces cas n'isolent ni la mesure de texte ni le
coût de recherche des collisions et n'exercent pas de cycle géométrique.

Le benchmark `diagnostic_geometry_256` dérive un masque Combined et recherche
les régions libres sur une grille résolue de 16 par 16 rectangles. Il vérifie
la déduplication, l'absence de collisions/overflow et les mêmes régions libres.
Compile/bind/layout préparent la fixture hors mesure ; validation, dérivation,
requête, vérifications et libération des résultats sont chronométrées. Il ne
mesure ni reflow dense, ni mesure de texte, ni encodage des exporters.

La soustraction diagnostique de rectangles conserve au plus quatre parties
temporaires inline, sans allouer un `Vec` par comparaison réussie. Requêtes de
régions libres et masques debug partagent ce helper, avec le même ordre et les
mêmes budgets. Les listes retenues allouent encore ; ce n'est ni une limite
mémoire du processus ni une mesure de baisse du RSS ou de gain de débit.

La sélection des bounds debug et la déduplication par élément utilisent aussi
quatre cases fixes. Les masques Combined éliminent les bounds répétés par
élément ; les overlays conservent chaque classe sélectionnée dans l'ordre.

`SceneInspector::query_free_regions_controlled` accepte `OperationControl`.
Il vérifie l'annulation autour de la validation de scène et du filtrage/tri
final, et rapporte les soustractions de rectangles terminées en phase Preflight.
Ces passes de validation/tri ne sont pas interruptibles en interne. La requête
soustrait les bounds de collision et exclusions résolus, respecte les budgets
diagnostiques et ne lit jamais de masque debug ni ne modifie la scène.
L'annulation abandonne le résultat partiel ; les observers synchrones doivent
revenir rapidement. Les méthodes existantes ne créent pas de token de contrôle.

`export_dataset_csv_controlled` accepte `OperationControl`, vérifie l'annulation
avant la sortie et aux frontières des lignes, et rapporte les lignes terminées
dans la phase Export. Une annulation retourne `Cancelled` ; le writer peut
contenir un préfixe CSV partiel à supprimer ou annuler par l'appelant. Les
callbacks dataset/writer sont coopératifs, pas préemptibles. Le pont AI utilise
le même contrôle pour CSV.

Le comptage du cache s'arrête dès que les octets sérialisés dépassent le budget,
sans encoder ni parcourir le reste de la scène uniquement pour la refuser.

Lors d'un miss, une capacité d'entrées/octets entièrement retenue par les
consommateurs est refusée avant le resolver, sans éviction. Les hits restent
disponibles. Ce précontrôle est conservateur si des leases sont libérés en
parallèle ; il ne réserve pas de scratch. L'insertion revalide l'admission.

L'admission SceneCache compte les scènes en cache et celles évincées encore
retenues par les consommateurs. `used_bytes()` indique les octets sérialisés
en cache ; `retired_bytes()` ceux observés des scènes évincées encore vivantes.
Les limites d'entrées/octets peuvent refuser une insertion tant qu'un ancien
Arc reste vivant. Des évictions FIFO peuvent précéder ce refus ; libérez les
anciens handles avant de réessayer. Le suivi faible ne retient pas les scènes
et reste borné par la capacité. Ce n'est pas un budget heap/RSS : scratch de
compilation, copies et allocations Arc::make_mut restent hors de ce contrôle.

Commencez par le
[guide YAML pas à pas](https://wiki.appcore.dnettoraw.com/fr/crates/appcore-filemaker-yaml).
Il construit progressivement un template V1 strict et fournit la référence
complète des champs racine et élément. Conservez
`appcore-filemaker schema --json` comme vérité exécutable du binaire installé.

Comparez ensuite l'[exemple de base](examples/basic.fr.md) et
l'[exemple intermédiaire](examples/intermediate.fr.md). La
[référence d'architecture et de contrats](architecture.fr.md) explique les
limites de l'engine.

Les couches de page sont parcourues paresseusement pour chaque page physique ;
la résolution par rôle ne crée pas de liste temporaire de références.
La planification des flux distribués utilise la même passe sans allocation pour
calculer l'espacement des enfants visibles.
Le fingerprint trie également les noms d'assets empruntés, sans cloner chaque
nom lors de la résolution déterministe.

Enregistrez les octets exacts des polices et un ordre de fallback avant la
mesure ; cet ordre entre dans le fingerprint et les exporters intègrent les
familles réellement choisies dans les glyph runs résolus. Appliquez les
patches runtime au binding, avant layout, afin que mesure, collision,
pagination et export utilisent une géométrie recalculée.
Le JSON du fingerprint utilise une passe de dimensionnement suivie d'un hachage
direct sous le budget agrégé `max_output_bytes`. Il conserve le framing V1
exact sans retenir les octets JSON canoniques.

Pour le japonais vertical ou une mise en page similaire, utilisez
`text_options.writing_mode: vertical`. Le moteur effectue la césure selon la
hauteur, façonne chaque colonne de haut en bas et avance les colonnes de droite
à gauche. Gardez `horizontal` (la valeur par défaut) pour le texte horizontal
et BiDi.

## Responsabilité de la mémoire raster

Les cas `raster_png_dense_rows_8` et `raster_png_dense_rows_256` rendent
4 096 rectangles et un fond. La fixture désactive les collisions pour isoler
l'export, pas le reflow. Quatre cas `raster_candidates_*` comparent sélection
linéaire et index expérimental par bandes de 8/256 lignes. Cet index existe
seulement dans le bench, limité à 65 536 associations ; construction et
libération sont chronométrées. Listes/ordre exacts sont vérifiés hors mesure,
comptages/checksums pendant. Le modèle couvre une page à 96 DPI et la marge
d'antialias, pas un index général de production.

Le benchmark runtime compare des bandes PNG/JPEG de 8, 64 et 256 lignes sur
la même scène FHD résolue (fond et 256 rectangles), écrite vers un sink. Cas :
`raster_png_rows_8/64/256` et `raster_jpeg_rows_8/64/256` (un suffixe numérique
par cas). Compile/bind/layout restent hors mesure ; validation d'export, rendu,
encodage et vérifications sont chronométrés. Cela mesure le compromis des
bandes, pas une implémentation d'index d'éléments ni le heap natif.

Pour PNG/JPEG, `export_raster_controlled` accepte `RasterOptions::new(bytes,
rows)` sans modifier `ExportRequest`. Les valeurs par défaut restent 4 Mio et
256 lignes ; limites acceptées : 1 octet–64 Mio et 1–4096 lignes, avec plafond
séparé de 4 Mio par scanline. Une scanline trop grande est rejetée avant encodage.
Par exemple, `RasterOptions::new(1024 * 1024, 64)?` borne la surface à
1 Mio/64 lignes. Des bandes plus petites peuvent répéter davantage de rendu,
notamment avec les blocs JPEG. Les formats non raster sont refusés ; layout,
pertes et overrides de peinture restent partagés. Exports existants, CLI/AI et
masques gardent les valeurs par défaut. Codec/assets/output et RSS sont distincts.

JPEG libère la bande précédente avant de rendre sa remplaçante, évitant deux
surfaces simultanées lors du remplacement du cache. En cas d'échec, le cache
reste vide et l'erreur est préservée. PNG transmet les bandes sans accumuler
la surface entière. Scratch du codec, assets décodés et buffers appelants
consomment de la mémoire supplémentaire ; le plafond d'une bande ne limite pas
le RSS du processus. Aucune baisse de RSS n'a été mesurée pour cette correction.

Les encodeurs internes refusent dimensions et hauteur de bande nulles avant
d'écrire ou d'appeler le renderer. Celui-ci vérifie le plafond de lignes prévu
avant allocation. Ces frontières sont défensives ; la validation publique
d'export reste applicable avant cette couche.

Après avoir résolu une scène, appelez `audit_layout` avec des limites de
ressources et `LayoutSafetyOptions` explicites. Le rapport est borné, expose
les comptes de débordement/collision/texte et peut refuser les avertissements
en mode strict. Sa représentation JSON est stable pour les fixtures golden et
les preuves de support; l’export conserve le preflight propre à chaque format.
