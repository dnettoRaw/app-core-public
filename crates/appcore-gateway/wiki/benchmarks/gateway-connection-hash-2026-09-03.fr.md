# Benchmark du hash de connexion Gateway — 2026-09-03

Le workload processus `appcore-dev bench --name appcore-gateway` a tourné sur
Apple M1 macOS/aarch64 avec Rust 1.97.1. Chaque résultat utilise cinq
échantillons release calibrés après un warmup écarté. Le cas worker lie 64
capabilities distinctes de 128 octets, la forme de connexion maximale acceptée.

| Cas | p50 avant | p50 final | p95 avant | p95 final | Évolution |
|---|---:|---:|---:|---:|---:|
| Hash de connexion client | 1,600 us | 1,288 us | 1,638 us | 1,326 us | p50 -19,54 % |
| Hash de connexion worker | 115,765 us | 106,769 us | 119,794 us | 108,556 us | p50 -7,77 % |

Le delta RSS du workload worker est passé de 0,34 à 0,27 Mio (-22,73 %) et le
delta retenu de 0,33 à 0,27 Mio (-19,05 %). Le delta du workload client est
passé de 0,19 à 0,09 Mio. Le framing maximal ne conserve plus son frame binaire
de 8,5 Kio ni une seconde chaîne payload de 17 Kio à côté de la sortie
hexadécimale finale requise.

Le comparateur a validé les six cas Gateway, témoins et modifiés. Ces mesures
sont locales au dépôt et ne constituent pas des claims production
multiplateformes.
