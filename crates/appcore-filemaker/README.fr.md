# appcore-filemaker

Les messages diagnostiques sont coupés uniquement aux frontières UTF-8 : les
erreurs conservent au plus 1 024 octets ; paths source, messages de validation
et pertes d'export au plus 512. Les grands buffers sont remplacés par le préfixe
borné. Cela borne le texte retenu, pas l'allocation préalable de l'appelant,
l'overhead de l'allocateur ni le nombre de rapports accumulés.

Pour PNG/JPEG, `export_raster_controlled` accepte `RasterOptions::new(bytes,
rows)` sans modifier `ExportRequest`. Les valeurs par défaut restent 4 Mio et
256 lignes ; limites acceptées : 1 octet–64 Mio et 1–4096 lignes, avec plafond
séparé de 4 Mio par scanline. Une scanline trop grande est rejetée avant encodage.
Par exemple, `RasterOptions::new(1024 * 1024, 64)?` borne la surface à
1 Mio/64 lignes. Des bandes plus petites peuvent répéter davantage de rendu,
notamment avec les blocs JPEG. Les formats non raster sont refusés ; layout,
pertes et overrides de peinture restent partagés. Exports existants, CLI/AI et
masques gardent les valeurs par défaut. Codec/assets/output et RSS sont distincts.

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

**BÊTA PUBLIQUE — `0.1.0-beta.2`.** Les API et le comportement peuvent évoluer
avant la version stable. Validez les sorties, limites et erreurs pour votre
charge ; l'implémentation et les tests locaux ne certifient pas la production.

[English](README.en.md) | [Português](README.pt.md)

Compilateur déterministe AppCore pour documents déclaratifs, canvases
vectoriels sémantiques et datasets bornés. Le YAML versionné
`filemaker: "1.0"` n'est qu'un frontend : compilation, liaison des données,
layout, collision, inspection, preflight et export restent des phases explicites.

Le crate utilise une géométrie fixed-point, des résolveurs explicites de
polices et d'assets, des ressources bornées, des scènes résolues immuables et
des erreurs typées. Le format est choisi lors de l'export, jamais dans le YAML.
Le bridge optionnel et la CLI restent dans des crates séparés.

Le shaping du texte utilise uniquement les octets de polices enregistrées.
L'ordre des fallbacks fait partie du fingerprint, et l'intégration SVG/HTML
suit les polices des glyph runs résolus. Les patches runtime sont appliqués
avant mesure et layout : la géométrie est donc recalculée depuis l'IR modifié.
Le JSON canonique du fingerprint est dimensionné et haché en deux passes writer
sous le budget agrégé `max_output_bytes` ; les octets V1 restent identiques sans
conserver un second buffer JSON complet.
`text_options.writing_mode: vertical` façonne des colonnes de haut en bas qui
progressent de droite à gauche. Mesure et césure ont lieu une fois dans le
layout ; PDF, SVG, PNG/JPEG et HTML consomment les mêmes colonnes et runs
façonnés.

Pour un processus long-lived, utilisez les constructeurs `OperationLog` et
`SceneCache` bornés en octets, `BorrowedDataset` pour les lignes déjà en mémoire
et l'API writer. PNG et JPEG rendent des bandes verticales bornées et les
encodent directement dans ce writer ; le PNG du masque de collision utilise le
même chemin. L'encodeur n'accumule pas toutes les bandes ni la sortie complète,
mais le writer appelant peut le faire. JPEG libère la bande précédente avant
de rendre sa remplaçante ; un échec ne laisse pas de surface périmée. Scratch
du codec, assets et buffers appelants restent séparés. CSV, SVG et HTML
streament aussi progressivement.
Les frontières internes refusent dimensions/hauteur de bande nulles avant
écriture/rendu et les bandes dépassant le plan avant d'allouer une surface.
PDF effectue une passe de dimensionnement bornée, puis émet des objets
indépendants et sa table de références croisées suivie sans conserver de buffer
final du document.
Le JSON, le SVG et le PDF du masque de collision suivent la même règle de
dimensionnement avant écriture et se sérialisent directement dans le writer de
l'appelant. PDF émet des objets indépendants, un content stream de taille exacte
et son xref classique sans retenir le stream de page ni le fichier complet ; le
helper JSON qui renvoie des octets dimensionne d'abord puis n'alloue que le
résultat exact accepté.

PDF prend en charge le texte éditable, flattened et hybride. Le mode hybride
dessine des contours de police déterministes pour l'apparence, puis ajoute une
couche Unicode invisible et subsettée pour la recherche, la sélection et
l'extraction, sans reflow dans l'exporter.
La planification des flux distribués compte les enfants visibles sans allouer
de liste temporaire de références, en conservant les mêmes calculs de taille et
d'espacement.
La collecte des noms d'assets du fingerprint trie des références empruntées,
évitant de cloner les chaînes lors de la résolution déterministe.

Le benchmark runtime du crate expose séparément `compile_canvas_yaml`,
`fingerprint_json_4m`, `collision_mask_json_4m`, `a4_report_end_to_end` et
`a4_report_pdf_hybrid`. `a4_report_export_matrix` exécute le même pipeline de
deux pages avec YAML/données/patch/mesure/layout/collision, puis préflight et
streame les trois modes PDF, SVG, HTML sémantique et fixe, PNG, JPEG et le CSV
du dataset vers des sinks sans rétention. Il a mesuré 70,56 ms p50, 71,34 ms
p95, 0,22 ms de MAD et 10,64 Mio de RSS de pic sur Apple M1.
`collision_mask_pdf_100k` écrit aussi un PDF de 1 800 626 octets depuis 100 000
rectangles résolus ; le cas JSON du masque écrit 4 188 826 octets dans un sink
sans rétention. La résolution des couches de page parcourt maintenant
paresseusement les éléments actifs de chaque page physique, sans liste
temporaire de références et avec le même ordre de rôles.

```bash
cargo run -p appcore-filemaker --example basic
cargo run -p appcore-filemaker --example intermediate
```

Chaque lanceur Rust charge un document `.yml` séparé dans `examples/` ; le YAML
du template n'est pas intégré au code Rust. Le lanceur de base écrit un SVG
complet d'une page ; l'intermédiaire écrit un PDF de deux pages, un HTML fixe,
des aperçus SVG par page et un rapport de preflight strict sous
`target/filemaker-examples/`. Les données typées restent aussi dans des JSON
séparés, et la police Noto Sans exacte sous OFL est fournie pour un résultat
portable et déterministe. Consultez
[l'architecture](wiki/architecture.fr.md), l'[exemple de base](wiki/examples/basic.fr.md)
et l'[exemple intermédiaire](wiki/examples/intermediate.fr.md).

Licence : MIT.

## Documentation stable

Identifiant stable : **ACR-023**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-023). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
