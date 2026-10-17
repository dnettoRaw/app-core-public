# Corpus mixte à contenu variable (1.0.0-rc.2)

La release candidate `1.0.0-rc.2` comprend les changements de contenu variable, hauteur intrinsèque, pagination adaptative, diagnostic, ordre de peinture et Unicode PDF ci-dessous. Les anciennes notes de source non publiée décrivent les étapes de développement. Sélectionner le package exact après vérification ; la migration du consommateur reste distincte.

Exécuter `cargo run -p appcore-filemaker --example variable_corpus --locked`.
Un dessin YAML et un plan adaptatif séparé reçoivent quatre datasets JSON typés
synthétiques. Les sorties `target/filemaker-examples/variable-corpus-*` incluent
PDF, scène, preflight strict, décisions, overlay et masques SVG.
Cet incrément corrige aussi la précédence de peinture des bandes dans le moteur existant. Aucune nouvelle dépendance, API publique, propriété sérialisée ou version publiée.

| Pages | Groupes | Lignes source |
|---|---|---|
| 1 | 3 | 9 |
| 2 | 12 | 36 |
| 5 | 40 | 120 |
| 20 | 190 | 570 |

Chaque groupe contient article, description variable et commentaire avec remise
détaillée. Les trois lignes partagent `keep_together_by` ; `oversized_group: error`
rejette explicitement une description dépassant une page. Le test vérifie le texte
logique après shaping et la hauteur mesurée des cellules, pas seulement les sources.
L’en-tête automatique à deux colonnes, le titre dépendant et le premier corps
partagent la mesure native. Trois taux de TVA, valeurs antérieures, remises,
totaux nets/fiscaux/finaux, conditions, signature et EAN-13 sont des données du
consommateur synthétique. FileMaker ne calcule pas les impôts et ne valide aucune
règle commerciale. Les totaux des fixtures sont cohérents, sans certification fiscale.

La première valeur antérieure est barrée par une ligne avec overlay local explicite.
Le filigrane de chaque page et le tampon initial utilisent les bandes existantes :
leur contrat de peinture sans collision rejette déjà un champ `collision` explicite.
La détection des collisions du corps reste active. Les ancres absentes doivent être
omises ou null, jamais des chaînes vides répétées. Les coordonnées X du corps sont
relatives à l’origine de la zone utile ; une régression vérifie que la barre reste
dans la cellule visée après cette translation. Il s’agissait d’une correction de
configuration de fixture, pas d’un défaut moteur ou de shaping.

Les tests vérifient ordre sans perte ni duplication, texte intégral, groupes
indivisibles, nombre de pages, géométrie de la barre et descendants de clôture
uniquement sur la dernière page, code et signature compris. `audit_layout` et
preflight strict sont exécutés. Les pages doivent aussi être rendues et inspectées :
zéro perte d’export ne valide pas le layout. Le défaut d’extraction des ligatures est corrigé dans la version 1.0.0-rc.2 : clusters complets et `/ActualText` contextuel conservent le texte logique. Vérifier séparément extraction et rendu. Les masques SVG montrent
l’occupation géométrique ; `audit_layout` applique les chevauchements intentionnels.

## Gate d’adoption du consommateur

Un consommateur fixé au paquet registry ne peut pas appeler des méthodes uniquement
présentes dans la source. Ne pas ajouter de patch local ni changer silencieusement
la dépendance avant publication et qualification d’une nouvelle RC indépendante.
Préparer l’adoption ainsi :

1. Sélectionner explicitement la nouvelle RC exacte du registry une fois disponible.
2. Déplacer identification variable et titre vers les racines du corps ; garder les
   bandes décoratives fixes. Définir `height: auto`, colonnes et ancres dans le YAML.
3. Remplacer les réserves fixes par un plan adaptatif, `first_after` et une racine
   finale. Retirer `reserve_after` concurrent et totaux internes du tableau dans ce
   dessin sélectionné ; les totaux restent des données liées.
4. Appeler `resolve_adaptive` ou `diagnose_adaptive`, puis audit et preflight stricts.
   Retirer mesure manuelle et substitution de coordonnées YAML seulement après les
   régressions synthétiques du consommateur réel.
5. Vérifier polices natives, en-têtes courts/longs, clôtures conditionnelles, remises,
   codes et sorties rendues. Conserver release, template et intégration antérieurs
   de l’application comme artefact explicite de rollback.

Le consommateur externe reste inchangé. Aucune publication de paquet ni déploiement wiki.

Reproduire les vérifications après l’exemple : `python3 crates/appcore-filemaker/wiki/audit/verify-variable-corpus.py`. Nécessite Poppler, vérifie IDs ordonnés, lignes de totaux fournies, 95 modules EAN-13 rendus et zones de silence, puis rend les 28 pages. Ne remplace ni revue visuelle ni certification scanner.

Le filigrane appartient à `page.master.background`. `z_index` ordonne la peinture dans une couche ; une valeur négative dans `header` ne place pas le filigrane derrière les glyphes du corps. La régression a d’abord échoué avec la bande d’en-tête, puis vérifie couche de fond et ordre de peinture. Le preflight strict acceptait la configuration masquant le texte.

Le corpus a aussi révélé un défaut moteur : le tri lexical plaçait la couche vide du corps avant `!page-background`, et des noms Unicode pouvaient dépasser la bande de premier plan. La correction compatible donne une précédence explicite aux bandes : fond, corps, en-têtes/pieds. Les noms des couches du corps gardent leur ordre lexical, puis z-index et séquence. Les tests couvrent couches vides/personnalisées/Unicode, z-index extrêmes et pixels raster réels. Aucun remplacement d’index spatial ni nouvelle API.

Recalculer les scènes en cache lors de l’adoption de la correction de peinture ; ne pas réutiliser le cache d’un ancien binaire source. Le rollback sélectionne explicitement la release qualifiée antérieure de l’application/crate.
