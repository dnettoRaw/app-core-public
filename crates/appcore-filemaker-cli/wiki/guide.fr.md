# Guide appcore-filemaker-cli

Cet adaptateur de processus borné compile le même YAML strict et utilise la
même scène résolue que l'API Rust. Le format d'export est choisi uniquement par
la commande, jamais par le template.

Utilisez `check` pour valider le schéma, `validate` pour le layout lié,
`preflight` pour une request exporter et `render` pour une sortie atomique PDF,
SVG, PNG, JPEG, HTML ou CSV tabulaire. `inspect`, `explain`, `free-regions`,
`debug` et `mask` sont des frontières de diagnostic. `schema` et
`capabilities` sont en lecture seule. `migrate` est réservé et renvoie
unavailable sans modifier l'entrée.

`schema --json` expose couleurs typées, cascade de style complète, overrides
d'export limités à la peinture et indépendance layer/z-index de la collision.
Il liste aussi unités de coordonnées, primitives, commandes de path et
graphiques avancés préparés du Canvas ; les templates n'encodent jamais une
surface peinte en pixels.

Utilisez `debug TEMPLATE --grid 1|5|10|20 --view combined` pour l'overlay
complet et non mutant. `mask` exporte la géométrie
collision/layout/visual/combined en PNG, PDF, SVG ou JSON. Le JSON sépare
occupied, free, collisions et overflow. `inspect` et `explain` renvoient
géométrie source, anchors, région, mesure, collision, page/reflow et provenance.
`free-regions TEMPLATE --minimum-width 20pt --minimum-height 10pt` renvoie les
rectangles résolus et bornés pouvant contenir cette taille minimale.

`capabilities --json` expose les PDF editable, flattened et hybride. Hybrid
dessine des contours vectoriels déterministes et ajoute un texte Unicode
invisible et subsetté pour la recherche, la sélection et l'extraction. WebP,
XLSX, ZPL, ESC/POS, PDF/A, liens, bookmarks et accessibilité tagged restent
préparés. La
description couvre writer/bytes bornés, rapports de perte strict/best-effort,
DPI raster uniquement, métadonnées PDF déterministes et subsets de polices.

Passez les données via `--data`, les polices via `--font NAME=FILE` répétable,
leur ordre de fallback via `--font-fallback NAME` répétable et un sandbox via
`--assets-root`. Appliquez les fichiers de patch ordonnés avec `--patch FILE`.
Pour CSV utilisez `render TEMPLATE --format csv --table ELEMENT --output FILE` ;
avec une seule table, `--table` est facultatif. Utilisez `--json` pour
l'automatisation stable et conservez les codes de sortie non nuls.

Chaque commande émet un texte humain concis par défaut et un JSON stable avec
`--json`. La découverte des capabilities publie les codes 0 (succès), 2
(validation), 64 (usage), 65 (données), 66 (entrée absente), 69 (indisponible),
70 (software), 73 (création impossible), 74 (I/O), 75 (échec temporaire de
ressource) et 130 (annulé).
Les deux modes se terminent par une newline et partagent un plafond stdout de
512 Mio. Le pretty JSON est d'abord dimensionné puis sérialisé directement via
un buffer fixe de 16 Kio, sans seconde string complète pour l'automatisation.

Les écritures d'artifact utilisent un fichier temporaire exclusif, sync des
données et rename atomique. `render` et `mask` rejettent une sortie qui se
résout vers leur template d'entrée. `migrate` est indisponible et non mutant ;
une migration future ne pourra écrire sans nouveau flag et contrat explicites.

`check`, `validate` et `preflight` séparent les diagnostics schéma, layout
résolu et exporter. Le JSON inclut des issues bornées et `truncated` explicite ;
strict rejette les warnings et toute troncature échoue fermée. `schema --json`
liste aussi validation des données typées, entrées complètes du fingerprint et
cache immuable borné resolve-on-miss.

Les lectures de template, données et polices restent sur un seul handle ouvert
et s'arrêtent après `limit + 1` octets. Overlays de debug et masques réutilisent
les limites core de la commande, dont le budget de comparaisons et géométrie
conservée.
