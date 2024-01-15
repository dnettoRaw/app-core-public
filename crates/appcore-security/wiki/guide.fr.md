# appcore-security

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

La RC 1.0 ne possède aucun provider TPM ou hardware-backed. L'ADR 0005 décrit
une proposition additive 1.1 avec fallback explicite et preuves sur matériel
réel; le Runtime actuel ne revendique aucune protection matérielle.

**Maturité :** contrats RC stables; la production dépend du backend secret et
des contrôles du déploiement.
