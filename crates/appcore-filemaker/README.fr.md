# appcore-filemaker

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
même chemin, et le raster complet et la sortie encodée ne coexistent jamais en
mémoire. CSV, SVG et HTML streament aussi progressivement.
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
