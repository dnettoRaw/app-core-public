# Laboratoire de performance et rapport d'optimisation

[English](benchmarks.en.md) | [Português](benchmarks.pt.md) | [Guide](guide.fr.md)

Ce rapport compare les mêmes charges déterministes `perf_lab` avant et après le
durcissement alpha du 2026-08-22. La baseline est le run initial ; les valeurs
finales sont la médiane de cinq runs release. La section ressources déclare sa
baseline et son protocole séparés. C'est une preuve d'ingénierie, pas une
garantie portable ni un seuil CI.

## Reproduire les mesures

```bash
cargo bench -p appcore-ai --bench perf_lab --all-features
APPCORE_AI_BENCH_FORMAT=jsonl \
  cargo bench -p appcore-ai --bench perf_lab --all-features
cargo bench -p appcore-ai --bench alpha_harness --all-features
```

JSON Lines contient charge, itérations, débit, wall time et p50/p95/p99 en
nanosecondes. Données et bornes sont fixes, mais fréquence CPU et autres
processus ne le sont pas. Comparez les distributions et répétez sur le matériel
de déploiement au lieu d'utiliser une valeur comme SLO.

Hôte de référence : Apple M1 MacBookPro17,1, 16 Gio, Darwin arm64,
`rustc 1.97.1`, release. Le processus final a été lancé directement après le
build pour exclure la mémoire du compilateur. Avec la charge requête explicite
de 1 Mio, macOS a mesuré 11,4 Mio de RSS maximum et 6,5 Mio de peak footprint.

## Avant et après

| Charge | p50 initial | p50 final | Écart |
|---|---:|---:|---:|
| resolve lightweight avec hit | 583 ns | 500 ns | -14,2 % |
| route modèle absente | 583 ns | 542 ns | -7,0 % |
| backend chaud, 1 route | 2,250 us | 2,042 us | -9,2 % |
| backend chaud, 32 routes | 96,417 us | 21,958 us | **-77,2 %** |
| chargement froid modèle unique | 2,875 us | 2,625 us | -8,7 % |
| scheduler, 32 candidats | 4,834 us | 4,500 us | -6,9 % |
| artefact local complet, 1 Mio | 3,409 ms | 3,086 ms | -9,5 % |
| range local 4 Kio | 16,583 us | 24,667 us | +48,7 % |
| batch Candle, 1 item | 2,250 us | 2,375 us | +5,6 % |
| batch Candle, 8 items | 17,708 us | 18,708 us | +5,6 % |
| batch Candle, 32 items | 68,959 us | 31,041 us | **-55,0 %** |
| scheduler Swarm, 1 000 peers | 226,958 us | 218,625 us | -3,7 % |

Les petits écarts Candle batch 1/8 sont sous la microseconde plus le bruit et
restent visibles ; batch 32 montre le crossover vectorisé. La régression du
range est un coût de sécurité volontaire : chaque lecture ouvre sans suivre les liens
et revalide le handle, le type fichier régulier et la taille exacte contre la
substitution symlink/reparse.

| Composant | 1 | 32 | 128 |
|---|---:|---:|---:|
| candidats du registre modèle | 250 ns | 7,167 us | 27,833 us |
| cost scheduler | 125 ns | 4,500 us | 20,583 us |

Swarm est mesuré directement à 1/10/100/1 000 peers ; JSONL donne les valeurs
exactes. Le batching final a mesuré 458 ns, 625 ns, 875 ns, 1,417 us et
2,542 us p50 pour 1/2/4/8/16 items. Le petit training Candle de 64 exemples sur
deux époques a mesuré 311,750 us p50. Valider par emprunt une requête binaire de
1 Mio mesure 42 ns contre 19,958 us pour le contrôle avec clone explicite,
environ 475 fois ; ce contrôle démontre la copie retirée, pas une autre baseline.

Le chemin de ressources de production a été mesuré séparément sur le même
Apple M1 :

| Opération default-light `alpha_harness` | p50 avant | Médiane p50 finale | Écart |
|---|---:|---:|---:|
| resolve lightweight | 1,542 us | 875 ns | -43,3 % |
| snapshot partagé caché | 541 ns | 167 ns | -69,1 % |

| Opération ressource | p50 final | p95 | Signification |
|---|---:|---:|---|
| snapshot partagé caché | 167 ns | 208 ns | lecture normale requête/scheduler |
| échantillon dynamique forcé | 2,416 us | 3,208 us | refresh natif CPU/RAM/device ; diagnostic seulement |
| découverte statique indépendante | 2,833 us | 3,834 us | nouveau setup sampler/topologie |

Les valeurs hot path finales sont la médiane de cinq runs release consécutifs ;
la valeur avant est le run enregistré avant la modification. Le tableau
dynamique/statique vient du run all-feature séparé de `perf_lab`. L'ancien
snapshot ne collectait pas le détail CPU/RAM/devices : le gain prouve la
frontière de cache, pas des lectures natives plus rapides. Le sampling physique
reste hors du hot path.
Un test laisse le sampler inactif sans lecture et observe zéro appel au probe ;
le sampler ne possède ni thread de polling ni buffer d'historique.

## Hotspots et modifications

L'ordre initial était routes, Candle item par item, I/O artefact, scans
scheduler puis contention. Désormais :

- modalités/octets sont précalculés, le backend forcé utilise un lookup direct
  et les routes scorées n'exigent plus de scan O(n²) ;
- les parties empruntées sont revalidées sans cloner texte/image/document au
  début de chaque `resolve` ;
- les records modèle immuables sont partagés par `Arc` entre routes ;
- Candle exécute un matmul et softmax vectorisé pour un batch borné à 64 avec
  validation et erreur par item ;
- le probe matériel s'exécute hors mutex et les refresh concurrents partagent
  une mesure, y compris la mise en cache bornée des échecs ;
- les loads modèle sont single-flight avec état ready LRU borné et compteurs
  load/wait/hit/eviction/invalidation ;
- le batch dépend de la latence, pression, mémoire et borne du backend ;
- le store mémoire ne clone qu'un `Arc` sous lock puis copie hors lock ;
- registres, scheduler, résidence, loads, peers, claims et transferts sont bornés.

Aucun cache de résultat `resolve` n'a été ajouté : les sorties peuvent être
sensibles, non déterministes ou dépendre du backend. Seuls samples ressource,
artefacts vérifiés, loads prêts et métadonnées de résidence sont réutilisés,
avec invalidation ou capacité fixe.

## Preuves CPU, mémoire et concurrence

La suite finale mesurée a pris 1,14 s wall, 0,62 s user et 0,10 s system sur
l'hôte. Ce sont des totaux processus, pas des budgets par requête. Le stress
lance 20 000 requêtes lightweight, jusqu'à 1 000 000 via
`APPCORE_AI_SOAK_ITERATIONS`, puis vérifie télémétrie et gauges vides ; la
certification a exécuté 100 000 requêtes.

Les tests concurrents couvrent 100 requêtes partageant un load froid, 32
writers d'artefact, annulation/deadline avant dispatch, probe single-flight,
saturation des files et churn de 1 000 peers. Le fuzz couvre artefact natif,
frontières de contrat et décodage OpenAI-compatible borné.

La propriété mémoire logique est également bornée :

| Propriétaire | Borne default ou fixe | Allocation |
|---|---|---|
| requête/réponse | 1 Mio chacune, 16 parties, 3 tentatives | revalidation empruntée, sans deep clone dans `resolve` |
| execution queue | 8 actifs + 128 en attente | erreur capacity avant croissance |
| batcher | 32 clés, 256 total, 64/clé, 16/dispatch | backend peut réduire le dispatch ; Candle direct refuse après 64 |
| registres/planners | 4 096 modèles, 256 backends, 4 096 routes load/learned/résidentes, 256 réservations | maps fixes ; ready loads en LRU |
| artefact | maximum agrégé choisi par l'appelant | store mémoire partage `Arc` ; load rend une copie ; range alloue son range |
| Swarm | 4 096 peers, 64 devices et 1 024 artefacts/peer | metadata/transferts bornés ; modèle hors RPC générique |
| Candle | batch inference 64 ; training 512, 4 096 dimensions, 256 classes, artefact 64 Mio | dataset paged/file-backed possible, un exemple borné à la fois |

Le nombre d'appels allocator n'est pas instrumenté : l'hôte n'a pas de profiler
dans le gate et le crate n'installe pas d'allocator global intrusif. Peak
mémoire, suppression
du deep clone et bornes logiques ont des preuves ; le profil allocations reste
une preuve externe requise pour RC/certification.

## Limites d'interprétation

Le benchmark HTTP exclut réseau et modèle réel. Candle utilise un petit
classifieur linéaire, pas un LLM. Le cache filesystem influence les artefacts.
Startup GPU/NPU, probes NVIDIA/AMD physiques, moteurs GGUF/MLX réels, tokens/s,
énergie, thermique et queues réseau doivent être mesurés sur le déploiement. Le
rapport matériel a tourné sur Apple M1 ; Linux/Windows et la feature NVIDIA ont
une preuve de compilation/tests déterministes, pas certification physique.
