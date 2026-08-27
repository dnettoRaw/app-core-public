# appcore-security

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

L'alpha 1.5 ajoute `WindowsDpapiSecretKeyring`, disponible uniquement sous
Windows. Les enregistrements sont protégés pour l'utilisateur courant sur la
machine courante, conservent une ACL réservée au propriétaire et refusent les
reparse points. La composition sélectionne explicitement
`windows-dpapi-user-v1` avec `provider:active` ; aucun fallback vers le file
keyring ou vers le scope machine DPAPI n'existe. La certification Windows
réelle multi-utilisateur et multi-machine reste en attente dans AC-009 ; cette
préversion n'est donc pas encore une revendication de certification production.

La ligne stable 1.0 ne possède aucun provider TPM, DPAPI ou hardware-backed. La
sélection de la préversion 1.5 est explicite et ne modifie pas le keyring
historique.

**Maturité :** contrats RC stables; la production dépend du backend secret et
des contrôles du déploiement.
