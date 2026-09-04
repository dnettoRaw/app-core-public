# Exemple intermédiaire

[English](intermediate.en.md) | [Português](intermediate.pt.md) | [Base](basic.fr.md)

Exécutez `cargo run -p appcore-filemaker --example intermediate`. Il construit
un rapport confidentiel plausible de performance de service sur exactement deux
pages A4, avec numérotation `Page {page} of {pages}`, filigrane incliné répété,
cartes KPI, style conditionnel, deux formes de graphiques vectoriels et table de
première classe groupée qui continue sur la seconde page. Il exerce aussi les
tokens de thème hérités, une dépendance calculée, les anchors de guide, un patch
atomique, un `OperationLog` borné, le fingerprint déterministe, le cache de
scène, l'inspection et le preflight PDF strict. Le document complet est le
fichier séparé
[`examples/intermediate.yml`](../../examples/intermediate.yml), chargé par
[`examples/intermediate.rs`](../../examples/intermediate.rs), avec les données
typées dans
[`examples/intermediate-data.json`](../../examples/intermediate-data.json).
Le lanceur enregistre explicitement la police Noto Sans sous OFL fournie et
écrit un PDF éditable, un HTML fixe, les deux SVG de page et le rapport JSON de
preflight sous `target/filemaker-examples/`. Les graphiques utilisent des
primitives vectorielles sémantiques, car le nœud `chart` de première classe est
préparé mais pas encore une capacité 1.0 implémentée.

Pour les images, passez le même résolveur explicite à
`LayoutEngine::with_assets` et `ExportContext`. Fit, crop, foyer, aspect,
orientation EXIF et DPI effectif sont résolus avant l'export.

L'espace couleur est explicite et les tokens peuvent résoudre des couleurs
fonctionnelles :

```yaml
themes:
  print:
    tokens: { ink: "cmyk(1000000, 250000, 0, 100000)" }
theme: print
style:
  fill: { space: gray, value: 245 }
  stroke: $ink
  opacity: 900000
style_rules:
  - when: data.highlight == true
    style: { fill: "rgba(255, 220, 0, 192)" }
```

Les règles s'exécutent dans l'ordre au binding. Un patch transactionnel
`SetStyle` ultérieur peut changer tout champ de style mesuré avant le layout.
`ExportStyleOverride` est la dernière couche globale et expose seulement fill,
stroke, opacity et couleur texte ; aucun reflow n'est possible dans l'exporter.

Utilisez `FontManager::register_from(&resolver, "fonts/Body.ttf", max_bytes)`
avec un `MemoryResolver` explicite ou un `FileResolver` à racine canonique ;
aucune font de l'hôte n'est découverte automatiquement.

Un transform reste indépendant du format et fixed-point jusqu'au layout :

```yaml
transform:
  translate_x: 4mm
  rotate: 45
  scale_x: 1250000
  mirror: vertical
  origin_x: 50%
  origin_y: 50%
```

Les coordonnées Canvas peuvent combiner unités physiques, écran, relatives,
logiques et normalisées tout en conservant un path sémantique :

```yaml
- id: curve
  type: path
  x: 10px
  y: 2mm
  width: 50%
  height: 12lu
  path:
    - { command: move, x: 0norm, y: 1norm }
    - { command: curve, x1: 0.25norm, y1: 0norm, x2: 0.75norm, y2: 0norm, x: 1norm, y: 1norm }
    - { command: close }
```

L'overflow du texte reste indépendant du format et mesuré avant la collision :

```yaml
text_options:
  overflow: ellipsis
  max_lines: 2
  min_font_size: 8pt
  line_height: 1250000
```

Contraintes et anchors de guide restent également déclaratifs :

```yaml
guides: { column: 25% }
elements:
  - id: card
    type: rect
    constraints: { preferred_width: 40mm, min_width: 30mm, max_width: 50mm, aspect_ratio: 1600000 }
    align_y: center
    anchors: { left: "guide:column+2mm" }
```

Réservez une géométrie non peinte avant le layout avec une exclusion nommée :

```yaml
exclusions:
  header-clearance:
    x: 0pt
    y: 0pt
    width: 100%
    height: 18mm
    collides_with: [content]
```

Les éléments du groupe de collision `content` effectuent leur reflow contre ce
rectangle sur chaque page ; les exporters ne peignent pas l'exclusion.

Pour des enfants flow de taille fixe, ajoutez `distribute: space_between` (ou
`center`, `end`, `space_around`, `space_evenly`) au groupe.

Les pages document peuvent composer un master avec une layer propre au rôle.
Chaque layer possède des bandes background/header/footer sans collision, et la
numérotation n'est résolue qu'après pagination :

```yaml
page:
  preset: A4
  master:
    footer:
      - { id: number, type: text, text: "Page {page}/{pages}", x: 15mm, y: 280mm, width: 60mm, height: 5mm, style: { font: Body, font_size: 8pt } }
  first:
    header:
      - { id: first-title, type: rect, x: 15mm, y: 8mm, width: 180mm, height: 8mm }
  continuation:
    header:
      - { id: continued, type: rect, x: 15mm, y: 8mm, width: 180mm, height: 4mm }
  last:
    footer:
      - { id: final-rule, type: line, x: 15mm, y: 275mm, width: 180mm, height: 1pt }
```

Enregistrez la police exacte `Body` avant le layout, comme pour tout texte.

Une table de première classe conserve les lignes typées pour les étapes
ultérieures de mesure et pagination :

```yaml
- id: results
  type: table
  binding: data.rows
  table:
    columns:
      - { field: name, header: Name, width: { mode: flex, value: 1 } }
      - { field: amount, header: Amount, width: { mode: auto } }
    repeat_header: true
    total_fields: [amount]
    max_rows: 1000
    max_cell_bytes: 4096
    row_height: auto
```

Les limites locales peuvent être plus strictes que `ResourceLimits`, jamais
plus grandes.

Après résolution, debug et inspection restent en lecture seule :

```rust
let explanation = SceneInspector::new(&scene).explain_layout(&ElementId::new("curve")?)?;
let mask = CollisionMask::derive(&scene, 0, MaskView::Combined)?;
let json = mask.to_json()?;
```

L'explication conserve géométrie source, anchors, mesure, collision,
page/reflow et provenance. Le JSON du masque distingue géométrie occupée,
libre, de collision et overflow sans interroger les pixels rendus.

Pour une sortie mémoire bornée, utilisez la même request et le même context que
l'API writer :

```rust
let (bytes, outcome) = export_bytes(&scene, &request, &context)?;
assert_eq!(bytes.len(), outcome.bytes_written);
```

Utilisez `export` avec un writer de l'appelant pour une sortie directe. Le CSV
Dataset reste streamé ligne par ligne via `export_dataset_csv` ; utilisez
`BorrowedDataset::new(&rows)` quand les lignes appartiennent déjà au document.

Pour compile-once/render-many, calculez
`DocumentFingerprint::compute_with_patches` avec les template, données,
patches, assets et polices exacts, puis appelez
`LayoutEngine::resolve_cached`. Un fingerprint répété renvoie le même
`Arc<ResolvedScene>` immuable sans refaire le layout ; éviction FIFO bornée et
contrôle de version de l'engine gardent le cache explicite. Configurez les deux
budgets avec `SceneCache::with_byte_capacity(entries, bytes)`. Configurez
undo/redo avec `OperationLog::new_bounded(entries, bytes)` afin qu'un grand
document ne puisse pas épuiser le processus malgré un petit nombre d'entrées.
