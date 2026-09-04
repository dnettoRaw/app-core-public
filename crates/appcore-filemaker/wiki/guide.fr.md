# Guide appcore-filemaker

Commencez par le
[guide YAML pas à pas](https://wiki.appcore.dnettoraw.com/fr/crates/appcore-filemaker-yaml).
Il construit progressivement un template V1 strict et fournit la référence
complète des champs racine et élément. Conservez
`appcore-filemaker schema --json` comme vérité exécutable du binaire installé.

Comparez ensuite l'[exemple de base](examples/basic.fr.md) et
l'[exemple intermédiaire](examples/intermediate.fr.md). La
[référence d'architecture et de contrats](architecture.fr.md) explique les
limites de l'engine.

Les couches de page sont parcourues paresseusement pour chaque page physique ;
la résolution par rôle ne crée pas de liste temporaire de références.
La planification des flux distribués utilise la même passe sans allocation pour
calculer l'espacement des enfants visibles.
Le fingerprint trie également les noms d'assets empruntés, sans cloner chaque
nom lors de la résolution déterministe.

Enregistrez les octets exacts des polices et un ordre de fallback avant la
mesure ; cet ordre entre dans le fingerprint et les exporters intègrent les
familles réellement choisies dans les glyph runs résolus. Appliquez les
patches runtime au binding, avant layout, afin que mesure, collision,
pagination et export utilisent une géométrie recalculée.
Le JSON du fingerprint utilise une passe de dimensionnement suivie d'un hachage
direct sous le budget agrégé `max_output_bytes`. Il conserve le framing V1
exact sans retenir les octets JSON canoniques.

Pour le japonais vertical ou une mise en page similaire, utilisez
`text_options.writing_mode: vertical`. Le moteur effectue la césure selon la
hauteur, façonne chaque colonne de haut en bas et avance les colonnes de droite
à gauche. Gardez `horizontal` (la valeur par défaut) pour le texte horizontal
et BiDi.
