# appcore-security

Les limites bearer V1 sont exposées dans `appcore_security::token` : claims JSON
décodées de 64 KiB maximum, signature du provider de 256 KiB maximum et enveloppe
hexadécimale de 655 364 octets maximum. Les deux composants sont contrôlés avant
le décodage ou la cryptographie ; un dépassement renvoie
`CommandTokenError::InvalidFormat`. L'émission borne les champs et les échappements
JSON avant signature et rejette une sortie vide ou excessive du provider.
Ces limites de sécurité ne changent pas le format wire ; les anciens tokens
trop volumineux doivent être réémis avec des claims réduites. Les allocations
internes du provider et les buffers de l'appelant ne sont pas contrôlés par
cette frontière. HTTP peut imposer une limite inférieure. Les tokens ne doivent
pas transporter les payloads de l'application.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** contrats réutilisables d'authentification, token, secret et
policy.

**Dépendances internes :** `appcore-core`, `appcore-dnt`.

**API principale :** provider HashToken, claims, factory/validator command
token, request hash, `SecurityError`; références, resolvers, stores, bytes
effacés, file keyring, metadata/rotation, contrat Vault, peer credentials,
adapter key provider DNT, traits authentification et policy.

À utiliser pour authentification infrastructure et indirection des secrets. Les
tokens sont signés, pas chiffrés. Ne pas placer autorisation domaine, OAuth,
TLS entrant ou vault managé ici.

`HashTokenProvider::from_secret`, `with_secret` et `with_material` retournent
un `SecurityResult` et appliquent les mêmes invariants minimaux de secret et de
salts. `compute_request_hash` produit un SHA-256 marqué `v2:` sur des champs
séparés par domaine, encadrés par leur longueur et avec présence optionnelle
explicite. Les anciens hashes sans version sont rejetés; émetteurs et
validateurs doivent être mis à jour ensemble.

`RequestValidationDetailsRef` et `RequestPayloadRef` offrent un chemin additif
emprunté pour les requêtes en cours. `compute_borrowed_request_hash` conserve
exactement la sortie V2 tout en comptant et hashant directement le JSON structuré
en deux passes, sans garder un payload encodé complet. Le contrat owned reste
disponible pour compatibilité.

## Provider Windows DPAPI dans `1.0.2-rc`

`WindowsDpapiSecretKeyring` protège chaque enregistrement borné avec DPAPI non
interactif dans le scope utilisateur courant et machine courante. Le keyring
exige aussi une DACL protégée réservée au propriétaire, refuse symlinks,
junctions et autres reparse points, et efface les owners du texte en clair.
Sélectionnez explicitement `windows-dpapi-user-v1` ; un répertoire
`file-keyring-v1` existant est refusé par le marqueur de format, sans conversion
ni fallback.

Le même utilisateur sur la même machine peut restaurer une sauvegarde complète
du répertoire après déchiffrement et validation de tous les enregistrements. Un
autre utilisateur ou une autre machine doit échouer de façon fermée. La
certification réelle multi-utilisateur et multi-machine reste en attente dans
AC-009 ; le RC constitue une preuve d'implémentation préliminaire, pas une
certification production. Le comportement stable 1.0 ne change pas et la mise
à niveau est explicite.

**Maturité :** contrats RC stables; la production dépend du backend secret et
des contrôles du déploiement.
