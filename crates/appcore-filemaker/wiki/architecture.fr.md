# Architecture

`appcore-filemaker` compile `Template + Données + Patches` vers une IR typée,
mesure les ressources et polices explicites, résout layout/collision/reflow et
produit une scène immuable. Inspection, preflight et exporters consomment cette
scène sans modifier sa géométrie.

La géométrie utilise des microunités entières. Le YAML accepte uniquement
`filemaker: "1.0"`. Includes, ressources et polices utilisent des résolveurs
explicites confinés. `ResourceLimits` borne parsers, boucles, datasets, rasters
et sorties.

Le binding utilise un compteur d'éléments unique partagé entre racines,
descendants et expansion des repeats, avec annulation/progression coopérative
aux frontières d'élément. La recherche de collision possède son propre budget
total de comparaisons en plus de la limite de reflow ; une scène creuse ou
superposée hostile échoue donc fermée plutôt que d'exécuter un travail
quadratique sans borne. Les assets filesystem sont ouverts depuis une racine
canonique sans suivre un symlink/reparse point final substitué, lus sous la
limite d'octets puis revalidés dans le sandbox autour de la lecture.
L'annulation d'export est vérifiée avant tout octet visible dans le `Write` de
l'appelant.

Le core ne dépend jamais de `appcore-ai`. Le bridge optionnel traduit 20 outils
bornés vers le core ; la CLI utilise `appcore-args` et l'écriture atomique. Le
contrat couvre thèmes/tokens explicites, données calculées, paths sémantiques,
boîtes de page et intention image. La scène résolue conserve glyphes, commandes
de path, placement image, bounds distincts, provenance et métadonnées.

Canvas est un contrat de dessin sémantique, pas un tampon de pixels. Les
coordonnées acceptent `pt`, `px`, `mm`, `cm`, `in`, `%`, l'unité logique `lu`
et les valeurs `norm`/`normalized` bornées à `0..=1`. Les nœuds text, image,
line, rect, circle, ellipse, polygon, path et group conservent leur identité
dans l'IR et le layout ; les commandes de path sont move, line, courbe cubique
et close. Circle rejette des axes résolus inégaux. Safe area, presets,
layers/z-index, transforms et collision restent des entrées explicites et
orthogonales de la même scène fixed-point.

Les couleurs traversent YAML et l'IR sans perdre leur espace. La source peut
utiliser des noms stables, hex, `rgb`/`rgba`/`gray` entiers, `cmyk` en
millionièmes ou une valeur typée avec tag. Fill est la peinture de fond, stroke
avec sa largeur est la bordure, et opacity reste séparée. Les résolveurs mémoire
et filesystem à racine canonique implémentent les frontières asset, template et
font avec contrôle de traversal et limites d'octets du demandeur ; enregistrer
une font ne parcourt jamais le système.

L'ordre normatif du style est exécutable : defaults du moteur, theme actif,
template, style component/nommé/inline développé, `style_rules` conditionnelles
ordonnées au binding, `SetStyle` runtime transactionnel, puis
`ExportStyleOverride`. Les changements runtime précèdent la mesure. La couche
d'export est limitée à la peinture (fill, stroke, opacity et couleur texte),
donc un exporter ne modifie ni métriques de font, ni bounds de stroke, ni
layout. Layer et z-index trient la liste de peinture sans agir sur la collision.

Les métadonnées image sont résolues une fois pour assets raster et SVG. Contain
et scale-down conservent l'aspect par ratios en microunités fixed-point ; fill,
none à taille intrinsèque, crop, cover focal et orientation EXIF facultative
produisent des rectangles source, destination et clip immuables. Le preflight
calcule le DPI raster effectif après transform. SVG/HTML incorporent SVG ;
PDF/raster signalent sa rasterisation non prise en charge sans perte silencieuse.

La politique de collision suit une cascade déterministe du document à la page,
la région, le groupe puis l'élément. Le raccourci booléen `collision: false`
reste explicite, et l'index spatial reçoit le bound mesuré sélectionné — layout,
visuel ou intrinsèque — avant le reflow.

Les transforms sont également résolus avant la requête spatiale. Translation,
rotation en degrés entiers, échelle fixed-point, flip/mirror et origins
explicites se composent à travers les groupes. PDF, SVG, raster et HTML
partagent la même matrice résolue et ses bounds visuels et de collision.

L'intention de mise en page du texte traverse YAML, IR, mesure et export sans
reflow dans le renderer. `text_options.overflow` accepte `wrap`, `shrink`,
`ellipsis`, `clip`, `expand` ou `error`, avec `max_lines` borné,
`min_font_size` absolu et `line_height` fixed-point. L'expansion précède la
requête spatiale ; le clipping devient une géométrie résolue ; SVG et HTML
consomment les runs façonnés et tronqués plutôt que le littéral original.
`writing_mode: vertical` façonne des colonnes de haut en bas progressant de
droite à gauche, et tous les exporters graphiques consomment ces colonnes et
runs résolus. PDF et raster utilisent directement les avances des glyphes.
L'emoji couleur reste une perte explicite jusqu'à son implémentation.

Les contraintes géométriques sont résolues avant mesure et collision.
`constraints` porte minimum, préféré, maximum et ratio largeur/hauteur en
millionièmes ; `align_x` et `align_y` choisissent début, centre ou fin dans la
page/région/groupe actif. Les anchors ciblent le bord d'un élément déjà résolu
ou un guide nommé via `guide:nom[+offset]`. Coordonnées, plages et ratios
contradictoires échouent explicitement. Un move runtime efface
anchors/alignement ; un resize runtime efface les contraintes de taille.

Les conteneurs flow distribuent les enfants de taille fixe avec `start`,
`center`, `end`, `space_between`, `space_around` ou `space_evenly`. Toute
distribution autre que start exige une taille primaire explicite, préférée ou
dérivée du ratio pour chaque enfant visible. Overflow et mesure auto ambiguë
sont des erreurs typées.

Les `exclusions` nommées au niveau racine sont des rectangles relatifs à la
page, résolus en géométrie fixed-point avant le placement des éléments. Elles
ne sont pas peintes, doivent rester dans la trim box, se répètent sur chaque
page physique et initialisent l'index spatial avec une règle immobile de
priorité maximale. Les champs optionnels `group` et `collides_with` utilisent
le même contrat de collision symétrique que les éléments. Les politiques
push/error/next-page/shrink existantes restent responsables du reflow ; les
instances répétées partagent le budget géométrique global. Inspection, masques
de collision et requêtes de régions libres conservent l'exclusion résolue,
tandis que les exporters ne reçoivent aucun node à peindre.

La source de page stricte accepte les layers `master`, `first`, `continuation`
et `last`. Chaque layer contient des listes explicites `background`, `header`
et `footer`. Les éléments master se répètent sur chaque page ; une layer de rôle
est choisie après la pagination du corps ; et le texte `{page}`/`{pages}` n'est
remplacé qu'une fois le total borné connu. Ces éléments partagent composants,
thèmes/styles, binding, patches, mesure et exporters avec le corps, mais restent
dans des bandes de peinture sans collision. Tables, repeat et anchors vers
d'autres éléments y sont rejetés afin que la décoration ne repagine pas le
corps.
Le flag résolu `collidable` exclut ces overlays du preflight de collision, des
masques de collision et de la soustraction des régions libres sans retirer leur
peinture.

Le moteur de table consomme des streams `Dataset` redémarrables sans
matérialiser toute l'entrée. Colonnes fixed, auto à échantillon borné et flex
pondéré deviennent des largeurs exactes. Hauteurs fixes ou mesurées par callback
paginent avec capacité correcte du header initial/répété, limites de groupe,
styles conditionnels ordonnés et totaux integer/decimal/currency vérifiés sur la
dernière page seulement. Lignes, fields, octets cellule, steps, échantillons et
pages ont des bornes explicites.

Le frontend YAML strict n'accepte l'intention de table que sur un élément
`type: table` et exige un binding vers un tableau. Colonnes, groupement, totaux,
styles conditionnels, politique de header et taille de ligne passent dans
`TableIr` ; le binding valide des lignes object et conserve leurs valeurs
typées. Les limites du template peuvent seulement réduire les bornes globales
de lignes, fields et cellules.

Le layout consomme cette intention typée et émet un `ResolvedTableFragment` par
page physique de la scène. Largeurs finales, répétition du header, rectangles
des lignes et cellules, styles de règles data, continuité de groupe, géométrie
des totaux et texte façonné deviennent l'entrée immuable des exporters. Une
continuation respecte les limites de page et le placement de collision ; les
renderers ne mesurent ni ne repaginent la table.

PDF éditable/flattened/hybride, SVG, raster et HTML peignent maintenant ces fragments
directement. L'usage des polices PDF inclut tous les runs cellule ; SVG/HTML
incorporent les polices choisies par les styles data ; raster trace les mêmes
glyphs. Le HTML sémantique conserve table, header, body, ligne, groupe et footer
alors que le mode fixed emploie les mêmes dimensions résolues. Preflight valide
nombre de cellules, bounds, diagnostics et disponibilité des polices incorporées
pour les modes PDF editable et hybride.

Les capacités préparées restent explicites : la fidélité stricte renvoie
`FM-EXPORT-UNSUPPORTED`; best effort enregistre la perte exacte. Aucun renderer
n'effectue d'approximation silencieuse.

Le debug est dérivé uniquement après le layout. `DebugOverlay` prend en charge
les grilles exactes de 1/5/10/20 points, règles, coordonnées, IDs, bounds
distincts, anchors, régions résolues, safe area, géométrie de
collision/exclusion et crosshairs, sans entrer dans la liste de peinture. Les
masques collision/layout/visual/combined dérivent leurs propres rectangles
occupés et libres et exportent PNG, PDF, SVG ou un JSON stable
occupied/free/collisions/overflow. Chaque élément résolu conserve une trace
bornée des x/y/width/height source, anchors, région, géométrie proposée, mesure,
policy de collision héritée, page/reflow et provenance pour inspection
structurée et explications déterministes.
Les exports JSON, SVG et PDF du masque comptent d'abord sous `max_output_bytes`
sans retenir la sortie, puis sérialisent directement dans le writer de
l'appelant. PDF emploie l'émetteur partagé d'objets/xref et un stream de
commandes de taille exacte, sans buffer de page ni de fichier. Le rejet reste
antérieur à l'écriture ; l'API JSON de commodité pré-dimensionne son unique
allocation exacte de résultat.

Les options d'export sont propres au format. Le DPI affecte uniquement
PNG/JPEG et la qualité JPEG uniquement JPEG. PNG commence transparent et
conserve l'alpha ; JPEG compose sur blanc seulement après enregistrement de la
perte alpha du style ou de l'image raster. HTML déclare la capacité sémantique
uniquement en mode semantic. Les modes PDF editable, flattened et hybride
partagent des métadonnées title/creator/producer déterministes. Editable
incorpore les subsets exacts de glyphes et leurs maps Unicode. Hybrid peint les
mêmes contours déterministes que flattened, puis place un texte Unicode
invisible et subsetté aux coordonnées résolues des glyphes pour la recherche,
la sélection et l'extraction. Chaque format document écrit
vers un `Write` de l'appelant ou `export_bytes` borné ; CSV streame les lignes
et fournit aussi des bytes bornés. Liens, bookmarks, accessibilité
tagged, PDF/A, WebP, XLSX, ZPL et ESC/POS restent des contrats préparés nommés,
sans approximation silencieuse.

La validation possède quatre frontières explicites : schéma, données typées et
bindings, layout résolu, puis preflight conscient de l'exporter. Les rapports
conservent des warnings bornés ; la policy strict les rejette et toute
troncature échoue fermée. Le preflight prévoit les écarts asset/vector, CMYK,
alpha JPEG, DPI effectif, embedding de polices et accessibilité, en plus des
diagnostics glyphes, overflow et collision.

Le fingerprint déterministe cadre versions schéma/engine,
template/données/patches canoniques, digests des assets référencés et des
polices enregistrées. Les champs JSON canoniques passent par un writer de
dimensionnement puis directement dans SHA-256 sous le budget agrégé
`max_output_bytes`, en conservant le framing V1 sans buffer JSON complet.
`LayoutEngine::resolve_cached` ne résout qu'en cas de
miss du `SceneCache` borné, partage les scènes immuables pour render-many et
rejette les anciennes versions d'engine.
Le cache est borné à la fois par le nombre d'entrées et par les octets sérialisés
agrégés des scènes. `OperationLog::new_bounded` applique la même double limite
aux snapshots `Arc<DocumentIr>` ; undo et redo déplacent les documents au lieu
de les cloner à nouveau. `BorrowedDataset` parcourt une slice de lignes sans la
dupliquer.

Seule la page de table bornée courante conserve des copies des lignes source.
Le sink de layout la convertit immédiatement en `ResolvedTableFragment`, sans
accumuler des pages brutes à côté de la scène résolue. CSV emprunte les cellules
textuelles lorsque possible et écrit les échappements de guillemets par morceaux.

La composition raster utilise des bandes verticales d'au plus 256 lignes et
environ 4 Mio, avec un plafond distinct de 4 Mio par scanline. PNG transmet les
bandes dans l'ordre ; JPEG les demande via son parcours documenté en blocs 8x8,
et les masques de collision PNG réutilisent l'encodeur par bandes. Le renderer
continue à consommer uniquement la géométrie résolue, sans mesurer ni résoudre
les collisions. CSV streame les lignes. SVG et HTML effectuent une
passe de comptage bornée puis écrivent progressivement markup, texte échappé,
paths et assets base64, tout en rejetant la limite avant de toucher au writer.
PDF applique le même dimensionnement borné, émet un chunk d'objet indépendant
de `pdf-writer` à la fois, suit les offsets et écrit xref/trailer à la fin. Il ne
conserve aucun buffer final du document ; images décodées et subsets de polices
sont libérés progressivement.
Le SVG du masque de collision suit ce chemin de comptage/streaming et échappe
les IDs par morceaux ; son JSON dimensionne le pretty-JSON déterministe puis le
sérialise directement. Le workload `collision_mask_json_4m` mesure un masque
exact de 4 188 826 octets avec checkpoints RSS idle, pic et retenu.
Le PDF du masque écrit les commandes fixed-point directement dans son stream
déclaré puis termine le xref classique. `collision_mask_pdf_100k` mesure 100 000
rectangles et un PDF exact de 1 800 626 octets sous les mêmes checkpoints.

Les gates de fiabilité conservent des snapshots exacts du SVG visuel et du
masque de collision, ainsi que les properties de géométrie fixed-point. Des
cibles fuzz séparées exercent le pipeline YAML/bind/layout borné, les données
Unicode arbitraires et textes énormes, les assets raster corrompus, géométries
absurdes et overlaps, anchors circulaires, et graphes d'include malformés,
circulaires ou trop profonds ; une entrée malformée peut produire une erreur
typée, mais jamais un panic, une boucle infinie ou une allocation sans borne.

La frontière finale de scène publique est protégée indépendamment de la
compilation. Export et preflight rejettent une ancienne version d'engine, des
styles ou placements d'image malformés, un overflow de coordonnées et un
dépassement des budgets pages/éléments/paths/lignes/texte avant toute écriture.
Les API bornées d'overlay, dérivation/JSON de masque, collision et régions
libres consomment `max_preflight_comparisons` ; les raccourcis utilisent des
valeurs par défaut bornées. `ElementId` revalide pendant la désérialisation.
Les checkpoints de l'export contrôlé s'exécutent dans les véritables boucles
d'éléments du renderer ; l'annulation empêche toujours l'artifact préparé
d'atteindre l'appelant.
Le pipeline de polices explicites utilise le shaping maintenu `harfrust` et
`skrifa` pour validation, métriques et outlines ; l'audit final a supprimé les
dépendances non maintenues `rustybuzz` et `ttf-parser` sans découverte système.
Quand une police valide omet la capital height OS/2, la policy nommée du
descripteur PDF utilise ascent comme `CapHeight` ; les advances absentes restent
des erreurs typées.

Le benchmark runtime sépare la compilation ciblée du workflow A4 complet.
`a4_report_end_to_end` et `a4_report_pdf_hybrid` décodent le YAML et les données
maintenus sur deux pages, appliquent un patch transactionnel, résolvent
mesure/collision/reflow, exécutent le preflight strict et streament le PDF
éditable ou hybride vers un sink. `a4_report_export_matrix` réutilise ce
pipeline complet une fois par itération et couvre neuf sorties : PDF éditable,
flattened et hybride ; SVG ; HTML sémantique et fixe ; PNG ; JPEG avec pertes
best-effort explicites ; et CSV du dataset. Sur Apple M1, il a mesuré 70,56 ms
p50, 71,34 ms p95, 0,22 ms de MAD et 10,64 Mio de RSS de pic.
`appcore-dev bench` échantillonne chaque workload dans des processus isolés ;
son pic RSS n'est donc pas confondu avec le cas plus petit de compilation
Canvas.
