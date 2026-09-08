# appcore-filemaker-ai

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

**BÊTA PUBLIQUE — `0.1.0-beta.2`.** Les API et le comportement peuvent évoluer
avant la version stable. Validez les sorties, limites et erreurs pour votre
charge ; l'implémentation et les tests locaux ne certifient pas la production.

[English](README.en.md) | [Português](README.pt.md)

Bridge optionnel et borné entre `appcore-ai` et `appcore-filemaker`. Il garde
la policy du modèle, les schémas d'outils, les budgets d'appels, la validation
des mutations et l'accès aux artifacts hors du core déterministe FileMaker.

Tous les arguments utilisent des schémas fermés, les mutations résolvent un
candidat avant commit et les limites du bridge ne peuvent que restreindre les
`ResourceLimits` du core.
La taille sérialisée du résultat est écrite dans un compteur borné qui ne
conserve aucun octet et s'arrête à `max_result_bytes`, sans allouer un second
JSON complet.

Le cycle create/patch/inspect/validate/preview/debug-mask/export complet est
exécutable et contrôlé par la policy. Une session dataset peut exporter une
table choisie en CSV borné en mémoire ; les outils graphiques exigent toujours
une scène résolue.
La découverte des capabilities et l'export exposent le PDF éditable, flattened
et hybride ; hybrid combine contours vectoriels et texte Unicode invisible et
recherchable.
La découverte du schéma expose l'écriture `horizontal` et `vertical_rl`
implémentée ; seul l'emoji couleur reste une capability préparée.

Consultez le [guide](wiki/guide.fr.md), l'[exemple de base](wiki/examples/basic.fr.md)
et l'[exemple intermédiaire](wiki/examples/intermediate.fr.md).

Licence : MIT.

## Documentation stable

Identifiant stable : **ACR-024**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-024). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
