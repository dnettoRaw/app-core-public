# Migration des mises en page déclaratives

Ce guide décrit les ajouts YAML et API Rust du FileMaker bêta actuel. Figez la
version du crate et les fichiers de polices de chaque PDF de référence;
`losses=0` ne prouve pas la fidélité visuelle.

## Alignement et retrait du texte

- Remplacez l'alignement numérique par espaces/NBSP par
  `text_options.align_x` ou `table.columns[].align_x` (`start`, `center`,
  `end`). L'alignement utilise la largeur façonnée finale après césure et
  réduction.
- Utilisez `text_options.padding_inline` pour un retrait symétrique conservé
  après césure. Définissez `table.columns[].padding: { top, right, bottom,
  left }` pour les retraits par côté; ils doivent être absolus ou logiques et
  non négatifs. Utilisez `text_options.padding: { top, right, bottom, left }`
  pour les insets du bloc texte; ils réduisent la zone mesurée et sont partagés
  par les exports et la pagination du texte développé. Les valeurs absolues,
  logiques non négatives et les pourcentages sous 50 % sont acceptés; `auto`
  est rejeté.
- Les littéraux Rust initialisent les nouveaux champs listés dans la section de
  migration beta3 ci-dessous. Utilisez `Default` lorsqu'il est implémenté pour
  conserver le comportement par défaut. Les anciennes lignes sérialisées lisent
  `source_text` comme vide; reformez le texte avant sa pagination.

## Modifications des littéraux Rust depuis beta3

Le contrat YAML `filemaker: "1.0"` reçoit des champs facultatifs compatibles,
mais l'API Rust beta ne préserve pas la compatibilité source des littéraux publics
exhaustifs. Les ajouts beta3–beta5 détectés par `cargo-semver-checks` sont:

- `TextOptions`: `align_x`, `padding_inline`; `TextSourceOptions`:
  `align_x`, `padding_inline`, `padding`; `TextIr`: les mêmes trois; `TextLayout`:
  `paint_offset_y`, `align_x`, `padding_inline`, `padding`; `TextLine`:
  `source_text`.
- `ElementSource` et `ElementIr`: `keep_with_next`, `text_segments`.
- `StyleSource`, `Style` et `ComputedStyle`: `stroke_sides`, `line_height`,
  `underline`.
- `TableColumn` et `ResolvedTableColumn`: `align_x`, `padding`;
  `ResolvedTableCell`: `padding`.
- `TableSource`: `keep_together_by`, `row_anchor_field`, `page_bodies`;
  `TableSpec`: `keep_together_by`, `row_anchor_field`; `TableIr`: `page_bodies`;
  `TablePage`: `row_padding`.
- `TableStyleRuleSource` et `TableStyleRule`: `padding`, `padding_first_page`,
  `padding_continuation`, `text_offset_y`, `text_offset_y_first_page`,
  `text_offset_y_continuation`, `min_height`,
  `min_height_first_page`, `min_height_continuation`, `reserve_after`.

Les consommateurs Rust qui construisent directement ces valeurs publiques
doivent ajouter les champs listés (ou utiliser `..Default::default()` seulement
pour les types qui implémentent `Default`). Il s'agit d'une migration beta, pas
d'une garantie que les littéraux beta3 compilent sans modification. Le contrôle
semver classe beta3–beta5 comme une étape major de préversion; imposer une
compatibilité patch échoue sur les champs ajoutés.

## Pagination

- Reliez les éléments frères avec `keep_with_next` jusqu'à l'avant-dernier
  élément visible d'un flux vertical.
- Pour les lignes de tableau, fournissez une clé de métadonnées non nulle pour
  chaque ligne et définissez `table.keep_together_by: layout_group`. Les clés
  contiguës identiques restent sur une page si leur hauteur mesurée tient.
  `group_by` indique seulement les débuts et ne garantit pas le maintien.
  Les groupes trop grands se répartissent entre lignes complètes; chaque ligne
  doit tenir sur une page. Représentez un bloc logique plus haut qu'une page par
  plusieurs lignes composantes avec la même clé; placez les valeurs totalisées
  sur une seule ligne pour éviter leur double agrégation.
- Le texte horizontal `overflow: expand` d'un flux vertical se répartit aux
  limites des lignes façonnées. Une ligne trop haute échoue explicitement.
- Les ancres nommées d'un tableau paginé ciblent son dernier fragment physique.
  Pour viser une ligne, déclarez `row_anchor_field` et fournissez une chaîne
  unique et bornée dans cette métadonnée; par exemple
  `anchors: { top: 'lignes::totaux.bottom+4pt' }` cible la ligne `totaux` et
  suit sa page physique. Un champ absent ou `null` ne publie pas d'ancre.
- Pour garder un élément suivant sur la page d'une ligne ancrée, ajoutez
  `reserve_after` absolu positif à une règle conditionnelle correspondante.
  Consultez `examples/row-anchor-reserve.yml` et ses données associées.
- Une règle `conditional_styles` correspondante peut définir `min_height: 18pt`.
  La ligne prend le maximum entre la hauteur mesurée du contenu et les minimums
  applicables, permettant un espacement par type sans police excessive. Utilisez
  `min_height_first_page` et `min_height_continuation` si les zones de page
  nécessitent des espacements différents; la mesure suit la page de destination.
- Définissez `style.line_height: 1250000` dans un style conditionnel pour
  remplacer l'interligne partagé (ratio en millionièmes, de 500000 à 4000000).

Utilisez `padding` par côté sur les règles `conditional_styles` correspondantes
pour l'indentation visuelle. Le dernier padding correspondant l'emporte et
s'ajoute au padding de colonne; ne préfixez pas les données de ligne d'espaces.
Utilisez `padding_first_page` ou `padding_continuation` si une même ligne
nécessite des retraits différents selon la page de destination. En l'absence
d'une valeur spécifique, la règle utilise son `padding` comme valeur de repli.

## Limites restantes

Le rich text et les codes-barres (dont EAN13) ne sont pas pris en charge.
Le texte autonome accepte les retraits par côté de l'axe bloc via
`text_options.padding`; ils sont mesurés et exportés de façon cohérente. Les
bordures de cellule sélectionnent
leurs côtés avec `style.stroke_sides`; `style.underline` trace chaque ligne
horizontale mesurée. Utilisez `table.page_bodies.first` et `.continuation`
avec `offset_y` et `height` pour séparer les zones du tableau entre première
page et continuation. Si le modèle
en lignes convient, séparez les styles en lignes avec `conditional_styles` et
`keep_together_by`. Le Runtime met en page et exporte les données typées; les
calculs et libellés du domaine appartiennent à l'application. Les exemples de
documents propres à une application restent dans son dépôt, pas dans ce crate
générique.

Après migration, rasterisez et inspectez toutes les pages. Couvrez une, deux,
trois et quatre pages ou plus, des annotations près des coupures, du texte long,
des largeurs numériques variées, plusieurs groupes et les dernières lignes
proches du bas de page.
