# Ressources matérielles, admission et placement

[English](resources.en.md) | [Português](resources.pt.md) |
[Guide](guide.fr.md) | [Performance](benchmarks.fr.md)

Cette page documente la frontière de ressources de production livrée dans
`appcore-ai 0.1.0-beta.1`. La détection informe la politique ; elle ne modifie
jamais fréquences, ventilateurs, puissance, pilotes ou protections de l'OS.

```text
capacité machine -> disponibilité actuelle -> budget du mode AppCore
                 -> fit modèle/runtime/batch -> device exact
                 -> batching, résidence, entraînement et don bornés
```

## Exécuter le rapport matériel

```bash
cargo run -p appcore-ai --example hardware_report
```

Sous Linux ou Windows avec NVIDIA Management Library installée, activez le
probe NVIDIA en lecture seule :

```bash
cargo run -p appcore-ai --example hardware_report \
  --features accelerator-nvidia
```

Le rapport n'affiche que capacités et charges agrégées et bornées, topologie,
classes d'échec, budgets calculés et compteurs du sampler. Il n'affiche ni
identité de l'hôte, paths, messages du pilote, prompts ou secrets.

## Lire un snapshot réel

```rust
use appcore_ai::{HardwareProbe, SystemHardwareProbe};

let probe = SystemHardwareProbe::default();
let snapshot = probe.sample()?;

println!("CPU logiques : {:?}", snapshot.logical_cpus);
println!("RAM disponible : {:?}", snapshot.available_memory_bytes);
for device in &snapshot.devices {
    println!(
        "{}: {:?} {:?} libre={:?} charge={:?}",
        device.id,
        device.kind,
        device.capabilities.memory_kind,
        device.available_memory_bytes,
        device.utilization_percent,
    );
}
```

Le sampler par défaut est global au processus, à la demande et mis en cache une
seconde. Sans thread de polling, son coût CPU au repos est nul. Après
l'intervalle, un lecteur rafraîchit physiquement tandis que les autres
attendent le même résultat. Succès et échecs sont cachés pour éviter les storms.

Un intervalle indépendant est réservé au diagnostic ou à un besoin mesuré :

```rust
use appcore_ai::SystemHardwareProbe;
use std::time::Duration;

let probe = SystemHardwareProbe::with_sampling_interval(
    Duration::from_millis(500),
)?;
let fresh = probe.refresh()?; // rafraîchissement diagnostic explicite
let metrics = probe.metrics();
```

`refresh` contourne la fraîcheur, pas le single-flight. Ne l'appelez pas pour
chaque requête. `captured_at_unix_ms` est un wall clock de diagnostic ;
`ResourceGovernor` reçoit un `now_ms` monotone du caller. Ne mélangez pas ces
domaines temporels.

## Sémantique du snapshot

- `None` signifie inconnu ou indisponible, jamais zéro ni capacité illimitée.
- mémoire totale, disponible et utilisée suit la sémantique de l'OS ; ce n'est
  pas le RSS du processus.
- `process_cpu_percent` est normalisé sur la machine entière ; la charge hôte
  reste distincte.
- chaque accélérateur a un `DeviceId` stable durant la vie du probe. Le backend
  doit l'associer exactement au device de son runtime et valider son usage.
- `compatible_apis` indique une famille pilote/API détectée, pas la preuve
  qu'une combinaison modèle/backend peut démarrer.
- un device disparu ou un pilote perdu ne reçoit plus de nouveau placement.
- les échecs n'exposent que composant et classe stable : `Unavailable`,
  `PermissionDenied`, `Driver` ou `InvalidData`.

### Mémoire dédiée et unifiée

Un GPU discret possède une VRAM dédiée. Le fit utilise le GPU cible exact :
deux GPU de 8 Gio ne deviennent jamais un GPU fictif de 16 Gio.

Apple Silicon partage un pool unifié. RAM et allocations GPU consomment un
seul budget, vérifié une fois, et créent seulement un tier `Memory`, sans tier
`Vram` fictif. Une topologie inconnue reste inconnue.

## Matrice des plateformes

`Implémenté` signifie que le code existe dans la beta. `Exécuté ici` signifie
qu'il a tourné sur l'hôte Apple Silicon de référence ; cross-compiler n'est pas
une certification physique.

| Plateforme | CPU | RAM | GPU/VRAM | Pression/thermique | Preuve beta |
|---|---|---|---|---|---|
| macOS Apple Silicon | implémenté | implémenté, disponibilité conservative free + inactive | GPU Apple intégré, mémoire unifiée, famille Metal ; utilisation indisponible | indisponible | exécuté sur Apple M1 |
| macOS Intel | implémenté | implémenté | GPU générique indisponible | indisponible | compilé, non testé physiquement |
| Linux | `/proc` + topologie sysfs cachée | `MemAvailable` ; PSI si exposé | DRM sysfs AMD partiel ; fallback sysfs NVIDIA partiel | PSI mémoire ; thermique indisponible | cross-compilé, non testé physiquement |
| Linux + `accelerator-nvidia` | idem | idem | NVML : VRAM totale/libre/utilisée et utilisation par GPU NVIDIA exact | thermique indisponible | compilé, sans exécution NVIDIA physique |
| Windows | compteurs natifs Win32 CPU/processus/système | `GlobalMemoryStatusEx` | NVIDIA seulement avec NVML optionnel ; GPU générique indisponible | indisponible | cross-compilé, non testé physiquement |
| autres targets | CPU logique lorsque `std` la fournit | indisponible | indisponible | indisponible | snapshot `Unsupported` explicite |

Les contrats représentent les NPU, mais cette beta ne livre aucun probe NPU
portable et fiable. Elles restent indisponibles, jamais inventées.

L'implémentation utilise des lectures natives bornées : `/proc` et `/sys` sous
Linux, Mach/sysctl sous macOS et Win32 sous Windows. Aucun shell, sous-processus
WMI, appel vendor en écriture ou scanner périodique.

## Modes et protection dynamique

```rust
use appcore_ai::{
    AiContributionPolicy, AiResourceMode, ResourceGovernor,
    ResourceGovernorConfig, SystemHardwareProbe,
};

let governor = ResourceGovernor::new(
    SystemHardwareProbe::default(),
    ResourceGovernorConfig::default(),
    AiContributionPolicy::default(),
)?;
let pair = governor.budgets(AiResourceMode::Balanced, 0)?;
println!("local={:?} contribution={:?}", pair.local, pair.contribution);
```

| Mode | Plafond CPU/GPU | Marge de capacité | Intention de concurrence |
|---|---:|---:|---|
| `Eco` | 40 % | 30 % | un job |
| `Balanced` | 70 % | 20 % | moitié des workers calculés |
| `Performance` | 90 % | 10 % | jusqu'aux workers calculés |
| `Unrestricted` | 100 % | 0 % volontaire | toujours borné par les maxima configurés et la sécurité OS/pilote |
| `Custom` | plafonds CPU/RAM/VRAM/work/jobs du caller | plafond exact dans la disponibilité | borné par le caller |

Par défaut, le governor réserve aussi 256 Mio RAM, limite 64 workers et huit
jobs, et exige trois échantillons consécutifs pour entrer ou sortir de l'état
de pression. État thermique critique, CPU/GPU élevés, PSI mémoire, RAM/VRAM
faible, device unhealthy, file et jobs actifs alimentent l'admission. Sous
pression stable, pourcentages, mémoire, workers et jobs sont divisés par deux.
Ce sont des valeurs de politique, pas des promesses matérielles.

`Unrestricted` retire seulement la marge volontaire AppCore. Kernel, pilote,
firmware, contrôle thermique et limites électriques restent souverains.

## Fit du modèle et admission du device exact

Les backends déclarent les composantes de pointe modèle, runtime/contexte et
batch avant admission :

```rust
use appcore_ai::ResourceEstimateBreakdown;

let estimate = ResourceEstimateBreakdown {
    model_memory_bytes: 6 * 1024 * 1024 * 1024,
    runtime_memory_bytes: 512 * 1024 * 1024,
    batch_memory_bytes: 256 * 1024 * 1024,
    model_vram_bytes: 6 * 1024 * 1024 * 1024,
    runtime_vram_bytes: 512 * 1024 * 1024,
    batch_vram_bytes: 256 * 1024 * 1024,
    cpu_percent: 30,
    gpu_percent: 80,
    workers: 2,
}.peak();
```

Le router appelle l'admission sur le device exact et combine charge et mémoire
actuelles aux métriques du backend. Le scheduler peut favoriser un modèle
résident et un moindre coût de transfert/activation, mais jamais contourner un
refus de capacité. Une capacité requise inconnue échoue fermée.

Le même budget limite `DynamicBatcher` via `BatchPressure::from_budget`, crée
des tiers de résidence sans recouvrement et fait réadmettre l'entraînement
avant chaque batch. Le batch diminue après admission limitée par pression et
s'arrête proprement sur defer/reject, sans exécuter un minimum à l'aveugle. Les
annonces Swarm sont à nouveau plafonnées par
`AiContributionPolicy` ; aucune capacité locale n'est donnée implicitement.

## Coût et justification de la feature NVIDIA

`accelerator-nvidia` ajoute optionnellement `nvml-wrapper 0.12.1`, wrapper sûr
MIT/Apache qui charge dynamiquement la NVML système. Le graphe par défaut n'en
dépend pas. Le target ajoute `libloading`, `nvml-wrapper-sys`, `bitflags`,
`static_assertions`, `thiserror` et du support de macros. Cette dépendance est
justifiée car les API standard de l'OS n'offrent pas de compteurs portables de
mémoire framebuffer et d'utilisation NVIDIA. AppCore utilise seulement des
queries et dégrade vers unknown/fallback sysfs si NVML ne démarre pas. La
feature n'installe aucun pilote.

Interfaces primaires utilisées :

- [compteurs `/proc` Linux](https://docs.kernel.org/filesystems/proc.html)
- [sysfs AMDGPU](https://docs.kernel.org/6.12/gpu/amdgpu/thermal.html)
- [mémoire Windows](https://learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex)
- [statistiques Mach VM](https://developer.apple.com/documentation/kernel/vm_statistics64_data_t)
- [indication de mémoire unifiée Apple](https://developer.apple.com/documentation/metal/mtldevice/hasunifiedmemory)
- [queries NVIDIA NVML](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html)

## Exploitation et métriques

`HardwareSamplerMetrics` expose `samples`, `sample_failures`, `cache_hits` et
`snapshot_age`. `ResourceGovernorMetrics` ajoute `admission_denied`,
`device_count` et les pressions CPU/mémoire. L'adaptateur hôte peut les mapper
vers `resource.samples`, `resource.sample_failures`, `resource.snapshot_age`,
`resource.cpu_pressure`, `resource.memory_pressure`, `resource.device_count`
et `resource.admission_denied` dans `appcore-ops`, sans DeviceId comme label.

Pour certifier la production, exécutez le rapport et le benchmark du modèle
réel sur chaque classe de déploiement. L'exemple OpenAI-compatible accepte
`APPCORE_AI_BENCH_ITERATIONS` ; il mesure complétion cold, débit warm et
snapshots, mais pas la latence first-token car le contrat reste non-streaming.
