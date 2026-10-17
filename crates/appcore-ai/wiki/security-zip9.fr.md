# Distribution de sécurité ZIP de Candle

`appcore-ai 1.0.0-rc.2` sélectionne `dnet-candle-core =1.0.0-rc.1` et `dnet-candle-nn =1.0.0-rc.1` sur crates.io pour le backend Candle optionnel. Ce sont des distributions de sécurité dérivées de Hugging Face Candle 0.11.0, pas des releases officielles upstream ni de nouveaux modules Runtime.

Le cœur officiel 0.11.0 exige ZIP 8.6.x, affecté par RUSTSEC-2026-0367, RUSTSEC-2026-0368 et RUSTSEC-2026-0369. Le cœur corrigé fixe `zip =9.0.4` ; NN sélectionne la même identité de cœur. Trois appels de noms NPZ/PTH propagent l’API faillible de ZIP 9. Les opérations tensor/NN restent identiques. SHA-256 des archives originales, révision Git, différences exactes du code et licences sont conservés dans le dépôt de distribution. Aucune exclusion d’alerte, patch local caché ou dépendance Git n’est distribué.

`backend-candle`, `training-candle`, format `NativeLinearV1`, IDs de backend et champs Rust publics restent inchangés. Le build par défaut reste sans framework ML. Les applications utilisent les contrats de données et la façade AppCore existants ; aucun tenseur Candle n’est ajouté à la façade publique. La validation CPU ne certifie pas CUDA/Metal/MKL.

Les régressions couvrent l’aller-retour NPZ Unicode, les noms anciens CP437 et les erreurs contrôlées d’archives malformées via les lecteurs NPZ immédiat/lazy et PTH. Les tests CPU upstream et AppCore AI avec toutes les features sont exécutés séparément. ZIP 9 conserve le décodage CP437 ; les octets UTF-8 invalides ne sont pas automatiquement rejetés comme erreur de nom.

L’adoption fixe explicitement `appcore-ai = "=1.0.0-rc.2"` après qualification du registre et actualise le lockfile, features optionnelles comprises. Vérifier `cargo tree --all-features -i zip` et `cargo audit`. Les pins exacts existants ne se mettent pas à jour seuls. Restaurer le graphe précédent complet exige d’évaluer ses vulnérabilités ZIP 8 connues. Préférer une release officielle Candle qualifiée lorsque son graphe publié supporte ZIP >=9 ; ne pas mélanger silencieusement les identités de cœur.

[Source and provenance](https://github.com/dnettoRaw/candle-zip9-security)
