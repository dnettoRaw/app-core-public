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

## Contrat générique de mise en page

Utilisez `text_options.align_x: start|center|end` pour le texte et `align_x`
sur une colonne du tableau. L'alignement utilise la mesure finale des polices
et s'applique aux exports PDF, SVG, raster et HTML. Une référence au tableau
paginer cible son dernier fragment. Chaque colonne accepte
`padding: { top, right, bottom, left }` avec des longueurs absolues ou logiques
non négatives. Ces retraits réduisent la zone de mesure des cellules et sont
partagés par les exports; les colonnes `auto` incluent les retraits horizontaux
dans leur largeur mesurée. Les règles conditionnelles peuvent aussi déclarer
`padding` par côté pour les cellules des lignes correspondantes. La dernière
règle correspondante qui déclare un padding l'emporte; ses retraits s'ajoutent
à ceux de la colonne et participent à la mesure, la pagination et l'export.
Pour composer une ligne de style partagé, `text_segments` accepte des parties
littérales ou liées à une chaîne et un `gap_after` facultatif. Utilisez
`text_options: { overflow: error, max_lines: 1 }` ; parties et espaces sont
mesurés comme une seule ligne alignée et préservés en PDF, SVG, raster et HTML.
Les segments ne se replient pas indépendamment ; le contenu multilignes ou à
styles mixtes utilise des éléments de flux séparés.
`padding_first_page` et `padding_continuation` peuvent remplacer ces insets par
rôle de page; une surcharge absente reprend la valeur `padding`.
Dans un flux vertical, `keep_with_next:
true` conserve un bloc contigu s'il tient sur une page. Dans un flux vertical,
un texte horizontal avec `overflow: expand` est réparti entre lignes façonnées
complètes lorsqu'il dépasse la zone de contenu; une ligne individuelle trop
haute échoue toujours. Les autres éléments ne sont pas répartis.

`group_by` indique le début de groupes, sans garantir une page commune.
`keep_together_by: layout_group` garde sur une page les lignes adjacentes dont
la clé non nulle est identique si leur hauteur totale tient; un groupe plus
grand est réparti entre lignes et chaque ligne doit tenir sur une page.

Pour viser une ligne précise, définissez `table.row_anchor_field` sur une
métadonnée contenant des chaînes uniques et bornées; utilisez
`table-id::nom.top` ou `table-id::nom.bottom` dans l'ancre. Une valeur absente
ou `null` ne publie pas d'ancre; les noms dupliqués sont refusés.
Les règles conditionnelles peuvent définir `reserve_after: 18pt` pour les lignes
ancrées correspondantes. Cette longueur absolue positive réserve de la capacité
de pagination au contenu suivant sans modifier la géométrie rendue de la ligne ;
la table doit déclarer `row_anchor_field`.
La fixture exécutable `examples/row-anchor-reserve.yml` avec
`examples/row-anchor-reserve-data.json` montre la ligne ancrée déplacée vers une
page de continuation avec l'élément qui la suit.

Pour différencier la zone du tableau sur la première page et les suivantes,
utilisez `table.page_bodies.first` et `.continuation`, chacune avec `offset_y`
et `height` relatifs à l'élément tableau. Les deux rectangles doivent tenir
dans cet élément ; les décalages négatifs et hauteurs non positives sont refusés.
Les styles conditionnels acceptent aussi `min_height` par ligne correspondante;
la pagination utilise le maximum entre le contenu mesuré et le minimum applicable.
Utilisez `min_height_first_page` / `min_height_continuation` lorsque
l'espacement varie entre la première page et les suivantes.
`style.line_height` remplace l'interligne partagé pour les styles correspondants,
y compris les lignes conditionnelles du tableau. C'est un ratio en millionièmes
de 500000 à 4000000; `line_height: 1250000` signifie 1,25.

Utilisez `text_options.padding_inline: 4pt` pour un remplissage symétrique sur
l'axe inline. La césure utilise la largeur intérieure réduite et conserve le
retrait sur chaque ligne, y compris dans les cellules du tableau.
Les éléments texte acceptent aussi un remplissage de bloc, par exemple
`text_options.padding: { top: 2pt, right: 4pt, bottom: 2pt, left: 4pt }`.
Ces insets réduisent les limites mesurées et sont conservés dans les exports
PDF, SVG, raster et HTML ; la pagination du texte développé réserve les mêmes
insets verticaux sur chaque fragment. Les longueurs absolues, logiques et les
pourcentages inférieurs à 50 % sont acceptés, sans valeurs négatives (`auto`
est rejeté) ; les pourcentages horizontaux utilisent la largeur de l'élément,
les verticaux sa hauteur.
L'indentation initiale d'un paragraphe est conservée sur les lignes de continuation après césure.

Les littéraux Rust de `TextOptions` initialisent `align_x` et `padding_inline`;
`TextLayout.padding` utilise `Insets::default()` et `TextSourceOptions.padding`
utilise `TextBlockPadding::default()` pour conserver le comportement précédent.
`TableColumn` initialise `align_x`. Le YAML des
colonnes utilise `start` par défaut. Les littéraux Rust de `TextLine` initialisent
aussi `source_text`; les anciennes scènes sérialisées lisent ce champ comme vide,
mais la pagination exige une mise en forme récente contenant les lignes source.

`keep_with_next` s'applique aux éléments frères dans un flux vertical. `style.underline`
trace un soulignement pour chaque ligne horizontale mise en forme, y compris les
styles conditionnels des cellules; il ne modifie pas la mesure et n'est pas rendu
pour le texte vertical. Configurez `style.stroke`, `style.stroke_width` et,
facultativement, `style.stroke_sides: { top: true, right: false, bottom: true, left: false }`
pour sélectionner les côtés des bordures de cellule; les côtés omis restent actifs.
Les règles conditionnelles acceptent aussi `text_offset_y`, une translation
verticale purement visuelle. Elle ne modifie ni le contenu mesuré, ni la hauteur
des lignes, ni la pagination et reste découpée aux limites intérieures d'origine
dans tous les exports visuels.
Utilisez `text_offset_y_first_page` ou `text_offset_y_continuation` pour la
remplacer selon le rôle physique de la page.
Le rich text, l'encodage EAN13 et
une composition arbitraire de groupes de valeurs ne
sont pas pris en charge. La limite de texte par défaut est de 4 Mio; `losses=0`
indique la prise en charge de l'exporteur, pas la fidélité visuelle.

**Bêta pré-1.0.** Validez les sorties, limites et erreurs pour votre charge;
les tests du package ne certifient pas la production.

[English](README.en.md) | [Português](README.pt.md)

Guide de migration : [English](wiki/migration.en.md) | [Português](wiki/migration.pt.md) | [Français](wiki/migration.fr.md)

Compilateur déterministe AppCore pour documents déclaratifs, canvases
vectoriels sémantiques et datasets bornés. Le YAML versionné
`filemaker: "1.0"` n'est qu'un frontend : compilation, liaison des données,
layout, collision, inspection, preflight et export restent des phases explicites.

Le crate utilise une géométrie fixed-point, des résolveurs explicites de
polices et d'assets, des ressources bornées, des scènes résolues immuables et
des erreurs typées. Le format est choisi lors de l'export, jamais dans le YAML.
Le bridge optionnel et la CLI restent dans des crates séparés.

Le shaping utilise uniquement des octets de police enregistrés explicitement
ou des métriques PDF Standard. L'ordre des fallbacks fait partie du fingerprint,
et l'intégration SVG/HTML suit les polices avec contours des glyph runs résolus.
Les patches runtime sont appliqués avant mesure et layout : la géométrie est
donc recalculée depuis l'IR modifié.
Le JSON canonique du fingerprint est dimensionné et haché en deux passes writer
sous le budget agrégé `max_output_bytes` ; les octets V1 restent identiques sans
conserver un second buffer JSON complet.
Pour un PDF éditable, `FontManager::register_pdf_standard` enregistre
explicitement une face latine PDF Standard 14. Les largeurs et le crénage AFM
pilotent la mise en page ; la sortie référence une face Type 1 sans rechercher
de police système ni l'incorporer. Cette voie est réservée au PDF et à WinAnsi ;
les caractères non représentables échouent explicitement ou utilisent un
fallback configuré. SVG, HTML, raster et PDF aplati exigent des contours
explicites. Symbol et ZapfDingbats ne sont pas encore pris en charge. Chaque
run Standard 14 est émis comme une opération de texte PDF native ; les
métriques AFM continuent de piloter la mise en page, tandis que le lecteur
applique les avances natives de la face lors du rendu.
```rust
fonts.register_pdf_standard("Helvetica", PdfStandardFont::Helvetica)?;
```
La licence et l'attribution des données AFM sont conservées dans
`LICENSE-APAFML` et `THIRD-PARTY-NOTICES.md`.
`text_options.writing_mode: vertical` façonne des colonnes de haut en bas qui
progressent de droite à gauche. Mesure et césure ont lieu une fois dans le
layout ; PDF, SVG, PNG/JPEG et HTML consomment les mêmes colonnes et runs
façonnés.

Le texte est limité par `ResourceLimits::max_text_bytes` (4 Mio par défaut) et
par un plafond absolu de 4 Mio dans le moteur. Il est possible de réduire la
limite configurée, mais l'augmenter ne relève pas le plafond du moteur. Un
texte trop volumineux est rejeté, jamais tronqué. Le texte développé hors
tableau peut continuer sur les pages suivantes aux limites de lignes composées.
Une ligne/cellule de tableau reste indivisible et doit tenir dans le corps de
page ; sinon la mise en page échoue au lieu de couper ou perdre le contenu.
Les styles distincts peuvent être composés avec des éléments/lignes séparés,
mais les runs mixtes dans un même nœud ou une même cellule ne font pas partie
du contrat YAML.

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
Pour adapter le code Rust entre versions beta, consultez le
[guide de migration du crate](wiki/migration.fr.md).

Utilisez `audit_layout` avec `LayoutSafetyOptions` après la résolution d’une
scène. Le `LayoutSafetyReport` borné résume débordements, collisions et
problèmes de texte, peut imposer une politique stricte sans avertissement et
produit un JSON déterministe pour les fixtures golden. Il réutilise les mêmes
contrôles de mesure, retour à la ligne, pagination et collision que l’export,
sans créer un second modèle géométrique.
