# appcore-provider-vercel-neon

Acquisition/renouvellement et libération de lease utilisent un seul essai HTTP,
quelle que soit la configuration des retries. V1 ne fournit pas de clé de
déduplication distante ; la réponse peut se perdre après application. Timeout
ou erreur HTTP transitoire ne prouvent pas que le lease est inchangé. Réconciliez
l'état autoritatif et le fencing avant les écritures dépendant du leadership,
sans rejouer aveuglément. Discovery, enregistrement et heartbeat conservent
leurs retries configurés ; la sémantique distante exige des tests de conformité.

La factory valide les retries avant de résoudre les secrets : 1–16 tentatives,
timeout et backoffs de 1 à 30 000 ms, avec un backoff initial inférieur ou égal
au maximum. La somme conservatrice `timeout × tentatives + max_backoff ×
(tentatives − 1)` ne doit pas dépasser 120 000 ms. Une configuration invalide
est rejetée, pas ajustée. Ce budget n'est pas un délai réel imposé à l'opération.

[English](README.en.md) | [Português](README.pt.md)

`appcore-provider-vercel-neon` est l’adaptateur officiel isolé qui permet à un
déploiement AppCore d’utiliser une API de control plane hébergée sur Vercel et
adossée à un service Neon exploité séparément. Les nœuds Runtime appellent
HTTPS ; ils ne se connectent jamais directement à Neon.

## Ce que fournit le crate

- `VercelNeonControlPlaneFactory`, enregistré sous
  `VERCEL_NEON_PROVIDER_ID` ;
- `SharedControlPlaneProvider`, le type remis à la racine de composition ;
- `AUTH_TOKEN_SECRET`, l’emplacement exact du secret attendu par la factory ;
- la validation de l’endpoint, des settings et du bearer token résolu avant de
  rendre le client de control plane disponible.

Le déploiement sélectionne explicitement le provider et fournit un endpoint
HTTPS avec une référence de secret :

```rust
use appcore_contracts::{ProviderConfig, ProviderId, SecretRef};
use appcore_provider_vercel_neon::{
    AUTH_TOKEN_SECRET, VERCEL_NEON_PROVIDER_ID,
};

let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID)?)
    .with_endpoint("https://control.example.com")?
    .with_secret_ref(
        AUTH_TOKEN_SECRET,
        SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
    )?
    .with_setting("timeout_ms", "5000")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Frontière de confiance

Le Deployment Manifest du Runtime ne contient que l’endpoint Vercel et une
référence de token. Les chaînes de connexion Neon, identifiants de base,
migrations de schéma, sauvegarde et rétention restent dans le service exploité
séparément. Secret absent, endpoint non HTTPS, setting invalide, échec
d’authentification ou service distant indisponible provoquent une erreur
explicite ; cet adaptateur ne choisit jamais un autre provider en repli.

Consultez l’[exemple de base](wiki/examples/basic.fr.md), l’[exemple
intermédiaire](wiki/examples/intermediate.fr.md) et le
[guide du crate](wiki/guide.fr.md). Exécutez :

```bash
cargo test -p appcore-provider-vercel-neon
```

## Documentation stable

Identifiant stable : **ACR-020**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-020). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
