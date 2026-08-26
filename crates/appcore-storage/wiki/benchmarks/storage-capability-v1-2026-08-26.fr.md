# Preuve du preflight des capacités storage V1 — 2026-08-26

[English](storage-capability-v1-2026-08-26.en.md) |
[Português](storage-capability-v1-2026-08-26.pt.md) |
[Guide](../guide.fr.md)

La certification release-profile clean-source a réussi sur macOS/aarch64 avec
Rust 1.97.1 au commit `12cbfc32264a57eb19b7e5c9e36ce076b3a1aee6`.

| Observation | Résultat | Gate |
|---|---:|---:|
| Types de capacité | 7 | exact |
| Capacité du catalogue | 32 | exact |
| Itérations preflight | 16 384 | exact |
| p50 | 42 ns | enregistré |
| p95 | 42 ns | enregistré |
| p99 | 83 ns | ≤ 1 000 000 ns |
| Débit | 10 493 879 ops/s | ≥ 10 000 ops/s |
| Exigence non supportée | échec fermé | obligatoire |
| Pic RSS de la suite | 320 464 KiB | ≤ 786 432 KiB |

Il n'existe pas de latence antérieure car l'ancien host n'exécutait aucun
preflight des capacités storage. Le baseline était l'absence de validation,
pas une opération équivalente plus rapide.

```bash
cargo run --release -p appcore-certification -- \
  bottlenecks builds/certification/ac016-bottlenecks.json
```

Cette preuve certifie le contrat de développement post-1.0. Elle ne modifie ni
ne republie les manifests V1 gelés.
