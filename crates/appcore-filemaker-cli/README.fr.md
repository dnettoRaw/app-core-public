# appcore-filemaker-cli

[English](README.en.md) | [Português](README.pt.md)

Adaptateur de ligne de commande borné pour `appcore-filemaker`. Il fournit les
commandes schema, validation, preflight, inspection, debug, mask et rendu
atomique avec une sortie JSON stable et des codes de sortie typés.
La sortie stdout humaine et pretty-JSON est dimensionnée sous un plafond de
512 Mio, puis écrite via des buffers fixes sans conserver une seconde `String`
complète.

La CLI applique des patches runtime JSON répétables, configure un ordre de
fallback explicite des polices, interroge les régions libres et exporte les
datasets tabulaires bornés en CSV sans les envoyer au layout graphique.
`render --format pdf --pdf-mode hybrid` écrit des contours déterministes et une
couche Unicode invisible et subsettée pour une sortie recherchable et
sélectionnable.
`schema --json` annonce `horizontal` et `vertical_rl` comme modes d'écriture
implémentés ; seul l'emoji couleur reste une capability préparée.

Les documents YAML et les données exécutables sont des fichiers séparés dans
`examples/` ; les exemples de commande ne cachent pas les templates dans du
code Rust ou shell.

Consultez le [guide](wiki/guide.fr.md), l'[exemple de base](wiki/examples/basic.fr.md)
et l'[exemple intermédiaire](wiki/examples/intermediate.fr.md).

Licence : MIT.
