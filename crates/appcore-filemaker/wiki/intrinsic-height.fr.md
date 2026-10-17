# Hauteur intrinsèque (opt-in, 1.0.0-rc.2)

La release candidate `1.0.0-rc.2` comprend les changements de contenu variable, hauteur intrinsèque, pagination adaptative, diagnostic, ordre de peinture et Unicode PDF ci-dessous. Les anciennes notes de source non publiée décrivent les étapes de développement. Sélectionner le package exact après vérification ; la migration du consommateur reste distincte.

Utiliser `LayoutEngine::resolve_intrinsic(&document)` au lieu de `resolve`
pour mesurer les éléments déclarant explicitement `height: auto` en YAML.
`resolve`, le parseur V1, les littéraux des structures publiques et les
politiques de dépassement à hauteur fixe conservent leur contrat. Aucune
nouvelle dépendance ni mesure manuelle côté application.

Le texte réutilise le shaping et la mesure existants avec la police enregistrée,
le style, la largeur, les marges internes et les avances de lignes effectifs.
Un flux horizontal prend la hauteur maximale ; un flux vertical additionne
les hauteurs et espacements. `constraints: {min_height: 14mm}` préserve le
format court. Flux et ancres reçoivent la boîte mesurée avant les collisions.
Le document d'entrée reste intact. Un contenu dépassant `max_height` échoue
avec son identifiant au lieu d'être coupé.

Cette première option accepte des largeurs définies et des flux du corps avec
distribution au début. Le texte automatique accepte `wrap`, `expand` ou
`error`, horizontal et sans `max_lines`. Déclarer une hauteur fixe pour `clip`,
`ellipsis` ou `shrink`. Les groupes automatiques sont indivisibles et doivent
tenir sur une page utile. Leurs enfants ne peuvent déclarer x/y, ancres,
région, transformations, alignement géométrique autre que début,
keep-with-next ou hauteur/contraintes de hauteur en pourcentage. Un texte
extensible enfant doit utiliser la hauteur automatique. Groupes absolus ou
distribués, écriture verticale, bandes de page automatiques, aspect ratio ou
preferred height sur un élément automatique et padding vertical en pourcentage
échouent explicitement, sans estimation circulaire implicite.

Le parcours vérifie l'annulation, les limites d'éléments/texte et une profondeur
maximale de 64 avant de cloner le document lié aux données. La mesure vérifie
l'annulation par élément ; le shaping d'un élément n'est pas préemptif. Copie
et mesure ajoutent du travail et de la mémoire, sans gain de performance promis.
Exécuter le preflight de layout sur la scène finale, y compris éléments fixes
et overlays. Ne pas réutiliser les clés de cache du layout ordinaire.

Exécuter `cargo run -p appcore-filemaker --example intrinsic_height --locked`.
`examples/intrinsic-height.yml` utilise des JSON court/long séparés, deux
colonnes, un texte aligné à droite et des ancres déplaçant titre et corps.
Résultats dans `target/filemaker-examples/`. Rust ne remplace aucune coordonnée
YAML. L'exemple exige des diagnostics de layout valides avant l'export PDF.

Migration : sélectionner la nouvelle méthode et déclarer les hauteurs auto et
minimales en YAML. Retour arrière : conserver le dessin original et `resolve`.
L'API Rust est additive, sans autre parseur/version YAML. Les réserves variables
de première/pages intermédiaires/dernière page de table utilisent désormais le plan adaptatif séparé ; les flux globaux arbitraires restent hors de ce contrat.

Dans une racine participant à la mesure intrinsèque, les groupes sont
structurels : les politiques de collision des descendants ignorent explicitement
les identifiants des ancêtres. Les collisions entre frères et avec les éléments
externes restent actives. La résolution ordinaire et les racines sans hauteur
automatique conservent leurs politiques.

Le défaut d’extraction des ligatures est corrigé dans la version 1.0.0-rc.2 : clusters complets et `/ActualText` contextuel conservent le texte logique. Vérifier séparément extraction et rendu.
