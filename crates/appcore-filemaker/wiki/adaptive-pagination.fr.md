# Pagination adaptative de table (opt-in, 1.0.0-rc.2)

La release candidate `1.0.0-rc.2` comprend les changements de contenu variable, hauteur intrinsèque, pagination adaptative, diagnostic, ordre de peinture et Unicode PDF ci-dessous. Les anciennes notes de source non publiée décrivent les étapes de développement. Sélectionner le package exact après vérification ; la migration du consommateur reste distincte.

`AdaptiveLayoutPlan::parse_yaml` lit un dessin YAML séparé et strict avec
`filemaker_layout: '1.0'`. Après le compile/bind V1 existant, appeler
`LayoutEngine::resolve_adaptive(&document, &plan)`. Cette API additive conserve
les littéraux des structures publiques, le parseur V1, `resolve` et
`resolve_intrinsic`. Aucune dépendance, règle métier ou format persistant ajouté.

Le plan sélectionne une table racine, les corps verticaux `first`,
`intermediate`, `last` et `single`, un bloc racine final `closing`, son espacement
et la politique obligatoire `oversized_group: error` ou `split_rows`. Offsets
et hauteurs sont relatifs à la boîte complète, à largeur constante. Tous les
corps doivent tenir dans cette boîte, elle-même dans la région utile de page.

Les corps déclarés précèdent la réserve de clôture. Sa hauteur visible réutilise
la mesure intrinsèque et les contraintes existantes. Hauteur et espacement
sont soustraits une seule fois de `single` ou `last`. Le paginateur existant
soustrait ensuite l'en-tête une seule fois. Une clôture masquée ne réserve ni
hauteur ni espacement. Les bandes décoratives conservent leur sémantique V1,
sans réserve implicite.

Le groupe en attente existant permet de reconnaître le groupe final. Seule sa
dernière ligne porte la différence entre capacité ordinaire et finale. Un
changement de page recalcule réserve et mesure sensible à la page au départ
suivant. Le fragment final est translaté vers le corps last/single ; les ancres
placent la clôture après son bord inférieur. Ce problème vertical restreint
ne nécessite ni estimation du nombre de pages ni point fixe global. Mesure de
groupes et échantillonnage des colonnes peuvent toujours revisiter les lignes ;
aucune garantie d'une seule lecture des données.

Les clés contiguës égales de `keep_together_by` restent ensemble quand groupe
et réserve finale tiennent sur une page de destination. `split_rows` sépare
les lignes si cette contrainte combinée dépasse la capacité ; `error` rejette.
Une ligne indivisible avec réserve trop haute échoue dans les deux cas. Un
groupe laissant la première page entièrement vide produit une erreur explicite,
sans page vide ni requalification implicite d'une continuation. Des données
vides peuvent produire en-tête et clôture si l'en-tête tient.

Limites explicites : une table racine visible commençant à la page physique
zéro ; un unique bloc final du corps ancré par `top: table-id.bottom+gap` ;
aucune transformation, région ou keep-with-next sur ces racines. La clôture
peut être un texte ou groupe de flux à hauteur automatique. Ses descendants
doivent tenir dans sa boîte mesurée sur la dernière page de table. Collision
incompatible ou croissance d'un texte fixe : erreur sans scène partielle.
Ne pas combiner `table.page_bodies`, `reserve_after` conditionnel ou
`total_fields` intégré avec ce propriétaire de réserve. Les totaux peuvent
être des données liées dans la clôture. Plusieurs tables, des largeurs finales
différentes ou un flux global dépendant des pages nécessitent un contrat
explicite ultérieur, sans inférence.

Les limites de taille, profondeur, éléments, lignes, pages et collisions restent
actives. Annulation vérifiée avant planification, pendant visite, mesure et
évaluation des réserves ; un shaping individuel n'est pas préemptif. L'index
spatial reste inchangé, sans gain de temps ou mémoire promis. Valider layout
et export séparément : réussite ou zéro perte déclarée ne prouve ni géométrie
ni fidélité du texte. Le défaut d’extraction des ligatures est corrigé dans la version 1.0.0-rc.2 : clusters complets et `/ActualText` contextuel conservent le texte logique. Vérifier séparément extraction et rendu.

Exécuter `cargo run -p appcore-filemaker --example adaptive_pagination --locked`.
`examples/adaptive-pagination.yml`, `adaptive-pagination-plan.yml` et les JSON
typés séparés génèrent 1/2/5/20 pages dans `target/filemaker-examples/`.
L'exemple exige audit de layout et preflight avant chaque PDF. La régression
de huit lignes passe de trois pages à deux avec une seule réserve finale ;
indices source uniques et ordonnés. Rust ne mesure pas le texte côté
application et ne remplace aucune coordonnée YAML.

Migration : garder le dessin original, déplacer les corps dans le plan,
retirer les réserves V1/par ligne concurrentes, déclarer l'ancre de clôture et
choisir la nouvelle méthode. Retour arrière : dessin et méthode d'origine ;
données et formats persistants inchangés. Inclus dans `1.0.0-rc.2` ; 
l'application externe n'a pas migré.

`first_after: {element: title, gap: 8pt}` place facultativement le début de
`first` et `single` sous une boîte de layout déjà mesurée à la page physique
zéro. Leurs bords inférieurs déclarés sont conservés ; le début ne remonte
jamais au-dessus de l'offset initial. Ceci relie en-tête automatique à deux
colonnes et titre à la première région, sans calcul de coordonnées côté
application. La boîte complète de table peut recouvrir les boîtes précédentes
alors que le corps utile sélectionné les évite : déclarer leurs identifiants
dans `collision.ignore` local, comme dans `examples/adaptive-header.yml`.
Aucune désactivation globale des collisions. Cible non résolue, ultérieure,
sur une autre page ou corps initial épuisé : erreur explicite. Un jeu non vide
doit placer des lignes sur la première page ; un report initial ne laissant
que des titres est rejeté pour éviter un titre isolé.

Exécuter `cargo run -p appcore-filemaker --example adaptive_header --locked`
pour les en-têtes court/long. Le YAML et le plan communs utilisent des JSON
synthétiques séparés, avec audit de layout et preflight obligatoires. Les deux
exemples produisent ensemble six documents PDF pour vérification visuelle.

## Preuves de pagination bornées (ajout 1.0.0-rc.2)

Utiliser `LayoutEngine::diagnose_adaptive(&document, &plan, max_decisions)` à la
place de `resolve_adaptive` pour obtenir une explication. `AdaptiveLayoutReport`
expose `outcome()`, `decisions()` et `into_outcome()`. Consulter ou enregistrer les
décisions avant de propager le résultat : les preuves précédentes restent
disponibles en cas d’échec. Chaque appel possède ses propres enregistrements.
Les API et littéraux publics existants sont conservés ; la résolution normale
ne crée aucun collecteur.

Chaque `PaginationDecision` contient l’ID du tableau, la page physique candidate,
les ordinaux des lignes source (base zéro, intervalle inclusif), la règle réelle
et les hauteurs numériques. `capacity` déduit l’en-tête du tableau une seule fois ;
`used_height` inclut les lignes et réserves précédentes ;
`available_height = capacity - used_height` ;
`required_height = group_height + reserved_height`. `final_reservation` est déjà
incluse dans `reserved_height`, sans addition supplémentaire. Elle combine bloc
final et espacement avec la différence entre hauteurs de corps normal et final,
et ne représente donc pas seulement la hauteur du bloc final.

Règles : `fit`, `move_group`, `split_rows`, `move_row`, `skip_first_page`, `reject`.
Un déplacement décrit le candidat source ; le `fit` suivant utilise les mesures
et réserves de destination. Une décision de groupe trop grand évalue une page
de continuation vide, même si elle n’est jamais émise. Ces preuves d’exécution
ne constituent ni une liste de pages émises ni une garantie de succès.
La validation préalable peut échouer sans décisions ; la validation finale peut
conserver des décisions `fit` sans retourner de scène. Un dataset vide ne génère
pas de décisions de lignes. L’enum des règles est non exhaustif.

La limite explicite doit être comprise entre un et `ResourceLimits.max_elements`.
Son épuisement retourne `LimitExceeded` avec le préfixe conservé, sans troncature
silencieuse ni scène partielle. Aucun texte, payload ou valeur de regroupement
n’est enregistré : seulement IDs, ordinaux et géométrie. Le collecteur optionnel
utilise de courtes sections mutex et conserve les caractéristiques Send/Sync du
moteur ; il ajoute un coût d’allocation et de sérialisation. Annulation et limites
existantes de pages, lignes et mesures restent actives.

`cargo run -p appcore-filemaker --example adaptive_pagination --locked` écrit
`*-pagination.json`, `*-debug.json` et les `*-mask-*.svg` par page, avec les exemples
de 1/2/5/20 pages. Réutilise `DebugOverlay::build_bounded` (zone utile, régions,
boîtes de layout/intrinsèques/visuelles et ancres) et l’exporteur SVG existant de
`CollisionMask`. Le masque représente l’occupation géométrique ; `audit_layout`
applique les politiques effectives de chevauchement.
`SceneInspector::explain_layout` explique la géométrie finale ; `audit_layout`
identifie collisions et coupures avec leur géométrie. Continuer le preflight strict
et la revue du PDF rendu : zéro perte d’export ne prouve pas un layout sûr.
Les étiquettes d’overlay peuvent contenir des expressions source ; les vérifier
avant partage externe.

Rollback : revenir à `resolve_adaptive` et ne plus enregistrer ces sorties.
Aucun changement YAML, persistance, dépendance, index spatial ou consommateur.

`header_height` indique la réserve d’en-tête appliquée ; `body_height = capacity + header_height` expose le corps normal avant cette déduction.
