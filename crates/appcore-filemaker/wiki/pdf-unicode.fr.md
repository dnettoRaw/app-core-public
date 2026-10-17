# Texte Unicode intégral dans les PDF (correction 1.0.0-rc.2)

La release candidate `1.0.0-rc.2` comprend les changements de contenu variable, hauteur intrinsèque, pagination adaptative, diagnostic, ordre de peinture et Unicode PDF ci-dessous. Les anciennes notes de source non publiée décrivent les étapes de développement. Sélectionner le package exact après vérification ; la migration du consommateur reste distincte.

L’export Editable et Hybrid associe chaque cluster à un seul glyphe à sa séquence
UTF-8 logique complète dans `ToUnicode`, au lieu du premier caractère seulement.
Le writer PDF existant produit les destinations UTF-16, paires de substitution
comprises. Les limites suivent les offsets logiques triés en octets, indépendamment
de l’ordre visuel RTL. Les frontières UTF-8 invalides et offsets terminaux ou hors
texte retournent `ExportUnsupported`, sans omission silencieuse de mappage.

Un glyphe peut représenter `fi` et le caractère littéral `ﬁ`, ou du texte composé
et décomposé. Un seul mappage par police ne représente pas toutes les occurrences.
Les runs ambigus ou clusters à plusieurs glyphes conservent le texte logique
original dans des spans PDF `/ActualText`. Le texte entier d’un cluster n’est pas
répété pour chacun de ses glyphes. Aucune normalisation Unicode n’est appliquée.
Les lecteurs de ces cas contextuels doivent respecter `/ActualText`.

PDF 1.7, polices, IDs, avances et géométrie sont conservés. Aucune nouvelle
dépendance, propriété publique, option YAML ou API. Standard 14 WinAnsi garde
son parcours existant. Flattened produit toujours des contours sans texte
recherchable. La correction concerne le mappage d’export et conserve layout,
shaping, politiques de coupure et positionnement bidi. Les extracteurs RTL peuvent
retourner l’ordre visuel avec contrôles directionnels ; l’exemple vérifie sa
séquence complète sans diacritiques en ordre logique ou visuel.

Les chaînes des CMaps partagent le budget explicite
`ResourceLimits.max_output_bytes`, vérifié avant allocation. L’index temporaire
des clusters est linéaire dans le nombre borné de glyphes ; tri et recherche
ajoutent du travail sans refaire le shaping. Les limites de scène, texte et sortie
restent actives, sans promesse de RSS constant ou de gain de vitesse.

Exécuter `cargo run -p appcore-filemaker --example pdf_unicode --locked` pour créer
les échantillons Editable, Hybrid et Flattened : `fi/fl/ffi/ffl`, mots courants,
ligatures littérales, accents composés/décomposés, cluster isolé à plusieurs glyphes
avec sa propre ressource de police et clusters arabes.

Puis exécuter
`python3 crates/appcore-filemaker/wiki/audit/verify-pdf-unicode.py`.
Nécessite Poppler, rend chaque mode, compare les scalaires latins exacts sans
normalisation NFC/NFKC, vérifie l’exemple RTL et l’absence de texte Flattened,
puis compare les pixels Hybrid/Flattened. Audit, preflight, extraction et revue
visuelle restent des vérifications distinctes.

Le vérificateur du corpus mixte contrôle maintenant tous les textes des éléments
et cellules, en plus des IDs et totaux. Les PDF existants doivent être réexportés.
Un consommateur registry reçoit cette correction via une nouvelle RC exacte publiée
et qualifiée. Aucune publication de paquet, déploiement wiki ou migration externe.
Le rollback sélectionne explicitement la release qualifiée précédente de l’exporteur.
