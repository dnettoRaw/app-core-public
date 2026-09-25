# appcore-security

Tests locaux:

```bash
cargo test -p appcore-security
```

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

**Responsabilité :** contrats réutilisables d'authentification, token, secret et
policy.

`create_private_directory` et `open_private_directory` fournissent une
frontière filesystem contrôlée par le propriétaire. Ils refusent les
composants symlink/reparse, valident les ancêtres inscriptibles, gardent les
handles ouverts pendant l'opération protégée et échouent fermé sur les
plateformes non prises en charge. Les permissions et ACL existantes ne sont
jamais réparées silencieusement.
Utilisez `PrivateDirectoryGuard::join` pour les chemins enfants ; il refuse les
chemins absolus, la traversée `.`/`..` et les caractères de contrôle avant que
le consommateur n'ouvre le résultat.

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

`CommandTokenValidator` refuse centralement une émission dans le futur, un
ordre temporel invalide et une durée de claims supérieure à
`TokenClaims::ttl_ms`. Les callers qui coordonnent des horloges distinctes
peuvent accepter au plus cinq minutes de skew positif à l'émission avec
`with_clock_skew_ms` ; l'expiration reste stricte.

La version `1.0.2-rc` ajoute `WindowsDpapiSecretKeyring`, disponible uniquement sous
Windows. Les enregistrements sont protégés pour l'utilisateur courant sur la
machine courante, conservent une ACL réservée au propriétaire et refusent les
reparse points. La composition sélectionne explicitement
`windows-dpapi-user-v1` avec `provider:active` ; aucun fallback vers le file
keyring ou vers le scope machine DPAPI n'existe. La certification Windows
réelle multi-utilisateur et multi-machine reste en attente dans AC-009 ; cette
préversion n'est donc pas encore une revendication de certification production.

Le package stable original `1.0.0` ne possédait aucun provider TPM, DPAPI ou
hardware-backed. La sélection du provider DPAPI additif dans `1.0.2-rc` est
explicite et ne modifie pas le comportement existant du file keyring.

**Maturité :** contrats RC stables; la production dépend du backend secret et
des contrôles du déploiement.

## Documentation stable

Identifiant stable : **ACR-010**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-010). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
