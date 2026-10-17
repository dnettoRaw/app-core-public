# Contenu variable : diagnostic et plan incrémental

> Mise à jour du code : l’incrément 1 corrige FMV-01/02/03 et le padding des groupes, avec régressions de région transformée et de marge après reflow. Les constats ci-dessous décrivent le baseline initial, pas le code corrigé non publié. Aucun paquet publié ni consommateur modifié. L’incrément 2 ajoute les conteneurs intrinsèques opt-in du corps ; l’incrément 3 ajoute les corps adaptatifs de table et la réserve finale de clôture. L’incrément 4 ajoute les preuves de pagination bornées optionnelles, conserve les décisions en cas d’échec et réutilise overlays et masques SVG.

Date : 2026-10-17. Source auditée : `appcore-filemaker 1.0.0-rc.1`.
État : diagnostic avant implémentation, sans prétendre avoir corrigé les constats.
Le diagnostic initial ci-dessous reste la preuve du baseline. Les corrections non publiées sont décrites ci-dessus ; le paquet publié et l’application consommatrice restent inchangés.
[English](variable-content-audit.en.md) | [Português](variable-content-audit.pt.md)

## Capacités à réutiliser

| Besoin | Contrat existant | Limite |
|---|---|---|
| Mesure identique à l’export | `TextEngine`, `FontManager`, `measure_content`, `TextLayout` | Polices explicites, shaping, largeur, padding et interligne sont déjà partagés. |
| Texte extensible | `text_options.overflow: expand` | Agrandit le texte, pas son conteneur parent ; voir FMV-01 pour la limite de lignes. |
| Placement dépendant | Flux vertical/horizontal et ancres vers les éléments précédents | Le parent ne prend pas automatiquement le maximum des hauteurs de ses colonnes. |
| Groupes cohérents | `keep_together_by`, `keep_with_next` | Clés contiguës ; un groupe trop grand se divise entre lignes/éléments, une ligne indivisible trop grande échoue. `group_by` n’est pas une garantie de cohésion. |
| Corps par page | `table.page_bodies.first` / `continuation` | Aucun `last` ; les champs inconnus sont rejetés. |
| Bandes de page | `page.master`, `first`, `continuation`, `last` | Peintes après pagination, sans réservation automatique. `last` exclut actuellement les documents d’une page. |
| Superpositions | `overlay`, `group`, `collides_with`, `ignore` locaux | Inutile de désactiver globalement les collisions ; voir FMV-03. |
| Géométrie et inspection | Bounds layout/intrinsic/visual, `SceneInspector`, `LayoutTrace`, `audit_layout`, `preflight`, `DebugOverlay`, `CollisionMask` | Réutiliser ces API ; les raisons complètes de chaque saut manquent. |
| Exécution bornée | Unités fixes, limites de ressources/comparaisons/reflows, annulation | La convergence globale devra disposer de son propre budget. |

Sources : [mesure](../src/layout_measure.rs), [texte](../src/text.rs),
[flux](../src/layout.rs), [groupes](../src/table_group_pagination.rs),
[régions](../src/ir.rs), [bandes](../src/layout_page.rs),
[collisions](../src/collision.rs), [audit](../src/validation_layout.rs),
[visualisation](../src/debug.rs).

## Configuration observée chez le consommateur

Lecture du modèle et de l’adaptateur fournis, sans données réelles.
Les deux colonnes déclarent 14mm, quatre lignes, une police de 9pt et un
interligne de 1,25. Quatre avances valent 45pt, environ 15,875mm : même quatre
lignes peuvent dépasser la boîte. `wrap` ne demande pas un agrandissement et
`min_font_size` n’active pas `shrink`. Les coordonnées absolues du titre/corps
ne suivent pas l’agrandissement. L’adaptateur mesure déjà les deux colonnes,
retient leur maximum et remplace des chaînes YAML : compensation fragile d’une
limite déclarative réelle.

Le tableau déclare 267mm, mais ses deux corps finissent à 260mm de la page :
`16 + 70 + 174` puis `16 + 244`. Les derniers 23mm de la boîte sont retirés sur
toutes les pages, indépendamment des blocs de clôture effectivement peints.
Ces éléments suivent le tableau ; le moteur ne déduit pas leur réservation.
`safe` n’est ni `margin` ni une réservation automatique. Le modèle utilise déjà
`keep_together_by: layout_group` ; faute de dataset représentatif, l’affectation
contiguë des mêmes clés aux descriptions/commentaires reste à vérifier.

Le consommateur désactive globalement les collisions et appelle `export_bytes`
sans `preflight` ni `audit_layout`. Zéro perte de format ne prouve pas la sûreté
du layout. L’intégration peut déjà utiliser ces API.

## Constats reproduits

| ID | Fixture | Résultat actuel | Classement |
|---|---|---|---|
| FMV-01 | [expand-cap-silent.yml](audit/expand-cap-silent.yml) | Six lignes deviennent quatre avec `expand` et `max_lines`, sans diagnostic ; zéro perte PDF et audit vide. | Défaut du moteur : preuve de perte absente. Préserver l’intention explicite de la limite. |
| FMV-02 | [next-page-margin.yml](audit/next-page-margin.yml) | `next_page` repart à `y=0` malgré une marge haute de 20pt ; l’audit accepte. | Défaut de reflow et lacune de validation de la région utile. |
| FMV-03 | [allowed-groups.yml](audit/allowed-groups.yml) | Placement autorisé entre groupes non interactifs, mais une collision à l’audit ; aucun débordement visuel. | Faux positif : appliquer `CollisionRule::applies_to` aux politiques effectives. |
| FMV-04 | [auto-container.yml](audit/auto-container.yml) | `height: auto` garde une hauteur par défaut ; l’élément ancré traverse la colonne la plus haute. | Limite du contrat, pas de mesure intrinsèque des enfants. Ne pas modifier silencieusement V1. |
| FMV-05 | [configured-cap.yml](audit/configured-cap.yml) | Coupe diagnostiquée, mais zéro perte d’export. | Configuration explicite et preflight non intégré, pas un défaut de shaping. |

Le reflow compare la hauteur physique et repart à zéro après dépassement ou
`next_page`. Tester aussi `push`, les régions et les bandes intentionnelles.
La mesure `keep_with_next` utilise `measured.height`, tandis que le placement
extensible utilise `outer_measured()` incluant le padding : incohérence à
reproduire avant de la déclarer confirmée. Les explications actuelles ne
retiennent pas capacité restante, hauteur du prochain groupe, réservations et
règle de chaque saut. Les diagnostics doivent rester bornés et sans contenu privé.

## Preuves et portée

55 tests existants passent dans `reflow`, `layout`, `table`, `pages`,
`debug_inspect` et `text`. Neuf scénarios synthétiques ont été exécutés ; leurs
PDF hybrides ont été rasterisés avec Poppler et examinés visuellement.
[Le modèle](audit/matrix.yml) et les jeux [1](audit/matrix-1.data.json),
[2](audit/matrix-2.data.json), [5](audit/matrix-5.data.json),
[20](audit/matrix-20.data.json) produisent exactement ces nombres de pages,
six lignes par page avec identification, description et commentaire. Les indices
source sont exactement `0..N`, sans perte, duplication ni réorganisation.
Ces contrôles courts à hauteur fixe ne qualifient pas encore l’auto-dimensionnement,
la dernière page adaptative, les descriptions extrêmes, TVA multiples, remises,
prix barrés, totaux, codes et filigranes combinés.

Preuves locales : `target/appcore/filemaker-layout-audit/` (PDF/PNG, scènes,
masques, JSON et logs). Ce ne sont pas des tests permanents qui figent les
défauts. L’index spatial linéaire reste inchangé ; pas de mesure release ni de
gain de performance revendiqué.

## Plan et gates

1. **Corrections compatibles.** Régressions désirées FMV-01/02/03 exécutées rouges
   avant chaque correction ; reproduire le padding des groupes. Conserver structs
   publiques, YAML et valeurs sérialisées. Tester push/next_page/shrink/error/overlay,
   ancres/resize, régions/marges/transformations/bandes. Mesurer l’index en release
   sur cas denses/épars avant remplacement. Gate : suite FileMaker complète,
   fmt/check/Clippy, rustdoc, consommateurs et baseline SemVer appropriée.
2. **Mesure intrinsèque opt-in.** Réutiliser le shaping : somme verticale et gaps,
   maximum des colonnes horizontales ; publier les ancres après mesure. Définir
   largeur/padding/interligne, texte vide, min/max et débordement indivisible.
   Rejeter les cycles de pourcentages sans base. Préserver 14mm comme minimum du
   design court. Coupe/expansion/réduction/erreur explicites, jamais de perte sans
   preuve. Même un champ public avec défaut casse les littéraux Rust : proposer
   types/entrée opt-in séparés, migration et rollback ; ne pas réinterpréter V1.
   RCs des crates toujours indépendantes. Gate : dessin court identique, textes
   longs complets et titre/corps pilotés en YAML sans remplacements Rust.
3. **Régions et convergence.** Réutiliser pagination/groupes, séparer page physique,
   contenu, réservations et peinture. Réserver une seule fois chaque bande active.
   Définir la page unique sans changer implicitement `PageRole::Last`. Groupe trop
   grand : division entre lignes ou erreur déclarée. Recalculer le rôle final avec
   unités fixes, ordre stable, annulation et budgets globaux de passes/mesures.
   Détecter les distributions/réservations répétées et échouer clairement, sans
   pages vides artificielles ni réservations doubles. Gate : 1/2/5/20 pages, limites
   de dernière page, aucun titre isolé, perte/réorganisation ou réserve inactive.
4. **Explications et sûreté.** Réutiliser audit/preflight/inspection/overlays.
   Rapport opt-in borné par saut : page, IDs/indices, région, hauteur disponible,
   groupe, réservations, règle et géométrie avant/après. Préserver les rapports
   publiés, nouvelle surface versionnée si nécessaire. Gate : JSON déterministe,
   troncature fail-closed, erreurs de cycle/budget utiles, export distinct du layout.
5. **Corpus complet et adoption.** Textes extrêmes, groupes, TVA multiples, remises
   détaillées, prix barrés, totaux, EAN/QR, tampons et filigranes synthétiques.
   Calcul fiscal dans les données de l’application. Régénérer et examiner toutes
   les pages PDF, comparer géométrie/pixels/ordre et HTML/SVG/raster de la même
   scène. Remplacer ensuite la compensation du consommateur, avec rollback et
   preuve d’équivalence du paquet registry. Gate final : tous les critères,
   documentation EN/PT/FR et RC indépendante qualifiée. Aucun paquet n’est publié
   ou déployé par cet audit.

## Reproduire avec la CLI existante

Exécuter à la racine de la workspace. Cette commande passe sans issue sur le contrôle de 20 pages. Réussir sur une fixture défectueuse ne prouve pas la sûreté ; comparer les constats ci-dessus.

```bash
cargo run -p appcore-filemaker-cli --locked -- \
  --json --strict \
  --font Body=crates/appcore-filemaker/examples/assets/NotoSans-Regular.ttf \
  --data crates/appcore-filemaker/wiki/audit/matrix-20.data.json \
  preflight crates/appcore-filemaker/wiki/audit/matrix.yml \
  --format pdf --pdf-mode hybrid
```

## Validation de l’incrément 1 — 2026-10-17

Les corrections compatibles sont implémentées dans le code non publié. Onze
nouveaux tests couvrent troncature, diagnostics des fragments, padding des groupes,
continuation dans marges/régions, transformations du parent, corps importé après
reflow, ordre overlay, pieds et ancres après shrink. Les échecs avant correction
sont conservés ; deux fixtures de fiabilité représentent désormais un chevauchement
importé non autorisé, pas un overlay permis. La suite FileMaker passe 236 tests ;
AI/CLI passent 47. Check/Clippy workspace, rustdoc, norm et formatage passent.
Comparaison patch avec `1.0.0-rc.1` publié : 223 checks SemVer applicables
réussissent (30 ignorés). Aucun champ public ou valeur protocolaire ajouté.

Les PDF de contrôle 1/2/5/20 pages régénérés sont identiques octet par octet
au baseline ; indices et identifiants PDF extraits restent complets et ordonnés.
Baseline local release : `reflow_dense_64` moyenne 1,412ms/itération ;
`diagnostic_geometry_256`, 1,852ms, dix itérations chacun. Workloads différents,
pas de comparaison de gain, étude de percentiles ou budget de production.
L’index linéaire reste inchangé. Preuves :
`target/appcore/filemaker-compatible-fixes/`. Les gates de hauteur intrinsèque,
convergence de dernière page et corpus commercial complet restent ouverts.

## Hauteur intrinsèque (opt-in, 1.0.0-rc.2)

[Hauteur intrinsèque (opt-in, 1.0.0-rc.2)](intrinsic-height.fr.md). `LayoutEngine::resolve_intrinsic` — `examples/intrinsic-height.yml`.

## Pagination adaptative de table (opt-in, 1.0.0-rc.2)

[Pagination adaptative de table (opt-in, 1.0.0-rc.2)](adaptive-pagination.fr.md). `AdaptiveLayoutPlan` / `LayoutEngine::resolve_adaptive`.

Diagnostic optionnel borné : `diagnose_adaptive` retourne les décisions numériques et conserve les preuves précédentes en cas d’échec. Voir le guide de pagination adaptative.

[Corpus synthétique mixte et gate d’adoption du consommateur](variable-content-corpus.fr.md).
