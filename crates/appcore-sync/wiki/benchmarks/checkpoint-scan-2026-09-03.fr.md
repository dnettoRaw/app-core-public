# Benchmark du checkpoint progressif — 2026-09-03

Le workload processus `appcore-dev bench --name appcore-sync` a été exécuté sur
Apple M1 macOS/aarch64 avec Rust 1.97.1. Chaque résultat utilise cinq mesures
release calibrées après un warmup écarté. Le fixture contient 32 768 IDs de peer
distincts de 128 octets et cherche le dernier peer tout en validant chaque
record V1.

| Mesure | Avant | Progressif | Variation |
|---|---:|---:|---:|
| p50 | 21,566 ms | 17,305 ms | -19,76 % |
| p95 | 21,731 ms | 17,418 ms | -19,85 % |
| RSS de pic | 21,50 Mio | 5,67 Mio | -73,62 % |
| Delta RSS du workload | 16,08 Mio | 0,25 Mio | -98,45 % |
| Delta RSS retenu | 16,08 Mio | 0,25 Mio | -98,45 % |

Le démarrage et le lookup parcourent maintenant le fichier avec un reader fixe
de 16 Kio sans conserver une map complète des peers. Une mutation possède
encore une map canonique triée afin de préserver les doublons et l'ordre V1,
mais l'écrit directement au lieu de construire une seconde string de la taille
du fichier. Les plafonds de fichier, ligne et records échouent fermés.

Ces mesures sont locales au dépôt et ne constituent pas une certification de
production multiplateforme.
