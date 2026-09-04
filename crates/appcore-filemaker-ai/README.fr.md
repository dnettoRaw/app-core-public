# appcore-filemaker-ai

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
