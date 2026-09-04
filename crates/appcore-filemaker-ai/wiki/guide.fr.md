# Guide appcore-filemaker-ai

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
