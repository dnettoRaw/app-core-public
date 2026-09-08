# Guide appcore-filemaker-ai

Les erreurs de conversion JSON conservent au plus 512 octets sans couper un
caractère UTF-8 ni garder un buffer surdimensionné. Un argument Unicode invalide
retourne une erreur contrôlée sans modifier document ou révision ; l'appel
rejeté consomme son budget. Serde peut avoir déjà alloué son message complet ;
ce n'est une garantie ni du pic d'allocation ni de redaction des secrets.

`filemaker_validate` compte aussi son enveloppe complète empruntée avant de
construire le JSON, messages, échappement du template et troncature compris.
Les warnings restent valides sans erreurs ni troncature ; un rapport tronqué
ne devient jamais `valid: true`. Le rapport du core est toujours construit
avant, dans sa limite d'issues : cela évite l'arbre JSON rejeté, pas le rapport.

L'admission des noms de tools et la validation de la liste autorisée utilisent
des contrats statiques d'arguments, sans reconstruire tous les schemas JSON
publics ni conserver un cache global. Pour la découverte du modèle,
`tool_definitions()` copie noms, descriptions et schemas sérialisés exacts depuis
les contrats statiques ; il ne construit plus d'arbres JSON ni de vecteurs
intermédiaires par catégorie. Les noms inconnus sont
rejetés avant le comptage des appels ; les tools connus conservent les mêmes
contrôles de policy, arguments et budget.

`filemaker_capabilities` compte une vue empruntée du contexte du document,
des limites et de la policy avant de construire l'arbre JSON. Un contexte
purpose/rules/editable/locked trop grand est rejeté sans cloner ces collections
vers JSON. Les réponses acceptées préservent champs, ordre des listes et compte
exact des octets échappés ; une session vide retourne toujours
`document_context: null`. L'arbre retourné reste owned, et les appels rejetés
consomment le budget d'appels. Cela ne borne ni résidence de session, ni
construction des diagnostics, ni scratch des exporters.

Le benchmark `runtime` inclut `create_patch_256_elements` : une session neuve
crée un Canvas de 256 rectangles, masque un élément par tool et en inspecte un
autre, en vérifiant révisions et résultats. Compile/bind et création du JSON
fixture sont hors chronométrage ; parsing JSON, conversion typée, deux layouts,
contrôles policy/résultat et destruction de session sont inclus. Il mesure
l'édition, pas shaping de texte, exporters, latence d'annulation ni tous les tools.

Les arguments typés de mutation, éléments source, longueurs et overrides de
style sont désérialisés depuis l'arbre JSON existant sans le cloner d'abord.
L'IR/patch produit possède toujours ses chaînes et collections ; ce n'est pas
un parsing sans copie du texte JSON original.

Les réponses de mutation sont dimensionnées avant le commit document/scène.
Si `max_result_bytes` ne suffit pas, create/load/patch et les outils d'édition
dérivés retournent une erreur de policy sans modifier document ni révision.
La tentative consomme le budget de calls ; la validation du candidat peut déjà
avoir eu lieu. Ce budget de réponse ne réserve pas la mémoire temporaire du layout.
Le plafond couvre le `ToolExecution` sérialisé complet, y compris `tool`,
`revision` et `value`. Les builders ne reçoivent que le budget exact restant
pour value ; un rejet de l'enveloppe externe ne survient donc pas après le
commit d'une mutation.

`export_dataset_csv_controlled` accepte `OperationControl`, vérifie l'annulation
avant la sortie et aux frontières des lignes, et rapporte les lignes terminées
dans la phase Export. Une annulation retourne `Cancelled` ; le writer peut
contenir un préfixe CSV partiel à supprimer ou annuler par l'appelant. Les
callbacks dataset/writer sont coopératifs, pas préemptibles. Le pont AI utilise
le même contrôle pour CSV.

Utilisez `FileMakerAiSession::with_control(OperationControl)` pour partager
annulation et progression avec layout/reflow, validation/preflight et export
graphique. Installez-le sur `empty(...)` avant create/load pour contrôler le
layout initial ; `new(...)` valide avant un appel ultérieur au builder. Les
appels annulés consomment le budget de calls. Un candidat annulé au layout
restaure scène et révision précédentes. Remplacer les contrôles ne réinitialise
ni policies ni budgets. Les requêtes de régions libres utilisent ce contrôle
après chaque soustraction de rectangles en phase Preflight. La validation de
scène et le filtrage/tri final ne sont pas interruptibles en interne. Le parsing
des arguments vérifie l'annulation avant, pendant un parcours sans allocation
par blocs de 16 Kio, puis après Serde. L'appel Serde borné reste indivisible et
peut consommer jusqu'au plafond configuré de 1 Mio avant le contrôle final. La
conversion des résultats et d'autres diagnostics conserve des lacunes ; les
callbacks/observers doivent revenir rapidement et ne sont pas interrompus de force.

Preview/export (CSV inclus) transmettent les octets de l'exporter via un scratch
base64 de 8 Kio vers la String résultat bornée, en ne gardant que deux octets
bruts au plus entre les writes plutôt que l'artefact brut complet. Métadonnées,
identifiants, échappement JSON et loss reports partagent le budget, revérifié
sur l'enveloppe complète. Cela ne borne pas le scratch de l'exporter ou du codec.

Les résultats typés inspect/explain, preflight, debug-mask et régions libres
sont comptés contre `max_result_bytes` avant conversion en Value JSON. Un résultat
trop grand s'arrête au comptage, sans arbre JSON supplémentaire. Le contrôle
final reste actif. Cela ne borne pas la scène résolue ni les DTO déjà construits
et ne remplace pas le comptage des artefacts/enveloppes base64.
L'inspection de page est sérialisée depuis une vue empruntée de la scène : les
noms exclusion/région et IDs en overflow sont parcourus directement, donc un
rejet ne clone pas d'abord ces listes dans `PageInspection`. La sortie acceptée
conserve exactement la forme JSON du core et possède les chaînes JSON finales.

Ce crate facultatif adapte les sessions déterministes `appcore-filemaker` aux
contrats d'outils bornés acceptés par `appcore-ai`. Il n'ajoute aucun
comportement IA au compilateur et ne laisse jamais un modèle choisir une sortie
filesystem.

Créez `FileMakerAiSession` avec `ResourceLimits`, polices, assets facultatifs et
`AiBridgePolicy` explicites. La policy borne appels, octets des arguments JSON,
opérations de patch et octets du résultat sérialisé. Les listes `ai.editable` et
`ai.locked` du template sont appliquées à tout subtree destructif avant une
modification atomique. Les purpose/rules textuelles forment un contexte compact
pour le modèle ; le bridge déterministe ne prétend pas interpréter le langage
naturel.
Le dimensionnement du résultat sérialise vers un compteur borné qui ne conserve
pas le payload et s'arrête dès que `max_result_bytes` serait dépassé, évitant
une seconde allocation JSON complète tout en gardant la frontière exacte.

Utilisez `tool_definitions()` dans `AiGenerationOptions`, puis transmettez les
appels exacts à `execute_call`. Les outils de requête sont en lecture seule. La
revision n'avance qu'après validation et, pour les modèles graphiques,
résolution réussies d'une copie candidate bornée. La séquence du patch est exactement la prochaine revision et
la limite effective d'opérations ne dépasse pas les `ResourceLimits` du core.
Export renvoie du base64 borné en mémoire.

`filemaker_export` accepte PDF, SVG, PNG, JPEG, HTML et CSV. CSV choisit une
table liée (ou exige son ID exact s'il y en a plusieurs) et parcourt les lignes
bornées directement depuis l'IR dataset. Une session dataset n'invente pas de
page ; preview, masques, régions libres et preflight graphique exigent toujours
une scène document/canvas.

Chaque déclaration d'outil possède un schéma fermé identique aux arguments
acceptés ; les champs inconnus échouent. Les capabilities exposent les appels
restants et un contexte document compact. `load` ne peut remplacer un document
de confiance et sa policy IA sans opt-in du host via
`allow_document_replacement`, faux par défaut.

`filemaker_schema` décrit couleurs typées et chaque couche de cascade. La
frontière bornée `filemaker_set`/patch accepte `set_style` transactionnel ; les
overrides de style d'export restent limités à la peinture et ne changent pas la
géométrie résolue.

`filemaker_add` accepte l'élément source strict et compact si l'objet possède
un champ `type`, y compris longueurs source, paths sémantiques, style,
transform, layer et collision. Un `ElementIr` complet avec `kind` reste
accepté. Le schéma annonce unités Canvas, primitives, commandes de path et
graphiques avancés préparés afin que le modèle n'invente pas d'opérations de
peinture pixel.

`filemaker_inspect` accepte un ID d'élément ou une page. Sa trace structurée et
`filemaker_explain` conservent géométrie source, anchors, région, mesure,
collision, page/reflow et provenance. `filemaker_debug_mask` déclare la page et
la vue collision/layout/visual/combined ; `filemaker_query_free_regions`
déclare ses dimensions minimales bornées.

Les capabilities exposent les PDF editable, flattened et hybride, puis nomment
séparément les fonctions PDF préparées restantes. Hybrid peint des contours
déterministes et une couche Unicode invisible et subsettée pour la recherche,
la sélection et l'extraction. La
description d'export garantit writer de l'appelant ou bytes bornés, rapport de
pertes strict/best-effort, DPI raster uniquement, métadonnées PDF déterministes
et subset de glyphes PDF ; le modèle ne doit pas déduire une sortie
indisponible.

`filemaker_validate` renvoie les issues layout bornées et la troncature
explicite. `filemaker_preflight` déclare format/fidelity/mode/page/DPI, strict
et policy d'accessibilité dans son schéma d'outil. Discovery nomme les étapes
schéma, données, layout et preflight, les entrées complètes du fingerprint et
le cache resolve-on-miss.

Les outils debug-mask et régions libres transmettent les limites core de la
session à la géométrie diagnostique bornée. Leur exécution ne peut donc pas
contourner le budget de comparaisons ou de géométrie conservée de la scène.

La session valide ensemble le document immuable et sa scène résolue. Les outils
de lecture clonent uniquement l'`Arc` de la scène sans refaire le layout. Un
patch construit et valide un seul candidat, puis remplace atomiquement les deux
valeurs ; un échec conserve le document et la géométrie précédents.
