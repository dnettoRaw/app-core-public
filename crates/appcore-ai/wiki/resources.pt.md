# Recursos de hardware, admissão e placement

[English](resources.en.md) | [Français](resources.fr.md) |
[Guia](guide.pt.md) | [Performance](benchmarks.pt.md)

Esta página documenta a fronteira de recursos de produção entregue no
`appcore-ai 0.1.0-beta.3`. A detecção informa a policy; ela nunca altera clocks,
ventoinhas, limites de potência, drivers ou proteções do sistema operacional.

```text
capacidade da máquina -> disponibilidade atual -> budget do modo AppCore
                      -> fit de modelo/runtime/batch -> device exato
                      -> batching, residency, treino e contribuição limitados
```

## Executar o relatório de hardware

```bash
cargo run -p appcore-ai --example hardware_report
```

No Linux ou Windows com NVIDIA Management Library instalada, habilite o probe
NVIDIA read-only:

```bash
cargo run -p appcore-ai --example hardware_report \
  --features accelerator-nvidia
```

O relatório imprime apenas capacidade e carga agregadas e limitadas,
topologia, classes de falha, budgets calculados e contadores do sampler. Não
imprime identidade do host, paths, mensagens do driver, prompts ou secrets.

## Ler um snapshot real

```rust
use appcore_ai::{HardwareProbe, SystemHardwareProbe};

let probe = SystemHardwareProbe::default();
let snapshot = probe.sample()?;

println!("CPU lógica: {:?}", snapshot.logical_cpus);
println!("RAM disponível: {:?}", snapshot.available_memory_bytes);
for device in &snapshot.devices {
    println!(
        "{}: {:?} {:?} livre={:?} carga={:?}",
        device.id,
        device.kind,
        device.capabilities.memory_kind,
        device.available_memory_bytes,
        device.utilization_percent,
    );
}
```

O sampler default é global no processo, on-demand e mantém cache por um
segundo. Ele não tem thread de polling, portanto seu custo de CPU ocioso é
zero. Depois do intervalo, um reader atualiza fisicamente enquanto os demais
esperam o mesmo resultado. Sucesso e falha são cacheados para evitar storms.

Use intervalo independente somente para diagnóstico ou requisito medido:

```rust
use appcore_ai::SystemHardwareProbe;
use std::time::Duration;

let probe = SystemHardwareProbe::with_sampling_interval(
    Duration::from_millis(500),
)?;
let fresh = probe.refresh()?; // refresh explícito de diagnóstico
let metrics = probe.metrics();
```

`refresh` ignora freshness, não single-flight. Não o chame em cada request.
`captured_at_unix_ms` usa wall clock para diagnóstico; `ResourceGovernor`
recebe `now_ms` monotônico do caller. Não misture esses domínios de tempo.

## Semântica do snapshot

- `None` significa desconhecido ou indisponível; nunca significa zero ou
  capacidade ilimitada.
- memória total, disponível e usada segue a semântica do SO, não o RSS do
  processo.
- `process_cpu_percent` é normalizado pela máquina inteira; carga do host é
  separada.
- cada acelerador tem `DeviceId` estável durante a vida do probe. O backend
  deve vincular exatamente esse ID ao device de seu runtime e validar o uso.
- `compatible_apis` indica uma família de driver/API detectada, não prova que
  uma combinação específica de modelo e backend inicializa.
- device desaparecido ou driver perdido deixa de receber novo placement.
- falhas expõem só componente e classe estável: `Unavailable`,
  `PermissionDenied`, `Driver` ou `InvalidData`.

### Memória dedicada e unificada

Uma GPU discreta possui VRAM dedicada. O fit usa a GPU exata: duas GPUs de
8 GiB nunca viram uma GPU fictícia de 16 GiB.

Apple Silicon usa um pool de memória unificada. RAM e alocações da GPU consomem
um único budget, são verificadas uma vez e geram apenas tier `Memory`, sem criar
um segundo tier `Vram` fictício. Topologia desconhecida continua desconhecida.

## Matriz por plataforma

`Implementado` significa código presente na beta. `Executado aqui` significa
rodado no host Apple Silicon de referência; cross-compilação não é certificação
física.

| Plataforma | CPU | RAM | GPU/VRAM | Pressão/térmica | Evidência beta |
|---|---|---|---|---|---|
| macOS Apple Silicon | implementado | implementado, disponibilidade conservadora free + inactive | GPU Apple integrada, memória unificada e família Metal; utilização indisponível | indisponível | executado em Apple M1 |
| macOS Intel | implementado | implementado | GPU genérica indisponível | indisponível | compilado, não testado fisicamente |
| Linux | `/proc` + topologia sysfs em cache | `MemAvailable`; PSI quando exposto | DRM sysfs AMD parcial; fallback sysfs NVIDIA parcial | PSI de memória; térmica indisponível | cross-compiled, não testado fisicamente |
| Linux + `accelerator-nvidia` | idem | idem | NVML: VRAM total/livre/usada e utilização por GPU NVIDIA exata | térmica indisponível | compilado, sem execução NVIDIA física |
| Windows | contadores nativos Win32 de CPU/processo/sistema | `GlobalMemoryStatusEx` | NVIDIA só com NVML opcional; GPU genérica indisponível | indisponível | cross-compiled, não testado fisicamente |
| outros targets | CPU lógica quando `std` informa | indisponível | indisponível | indisponível | snapshot `Unsupported` explícito |

Os contratos representam NPU, mas esta beta não entrega probe NPU portátil e
confiável. Ela fica indisponível, nunca é inventada.

A implementação usa leituras nativas limitadas: `/proc` e `/sys` no Linux,
Mach/sysctl no macOS e Win32 no Windows. Não há shell, subprocesso WMI,
chamada vendor com escrita nem scanner periódico.

## Modos e proteção dinâmica

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
println!("local={:?} contribuição={:?}", pair.local, pair.contribution);
```

| Modo | Teto CPU/GPU | Headroom de capacidade | Intenção de concorrência |
|---|---:|---:|---|
| `Eco` | 40% | 30% | um job |
| `Balanced` | 70% | 20% | metade dos workers calculados |
| `Performance` | 90% | 10% | até os workers calculados |
| `Unrestricted` | 100% | 0% voluntário | ainda limitado por máximos configurados e segurança SO/driver |
| `Custom` | tetos de CPU/RAM/VRAM/work/jobs do caller | teto exato dentro da disponibilidade | limitado pelo caller |

Por default, o governor ainda reserva 256 MiB de RAM, limita 64 workers e oito
jobs e exige três amostras consecutivas para entrar ou sair do estado de
pressão. Estado térmico crítico, CPU/GPU altas, PSI de memória, RAM/VRAM baixa,
device unhealthy, fila e jobs ativos alimentam a admissão. Sob pressão estável,
percentuais, memória, workers e jobs são reduzidos pela metade. São defaults de
policy, não promessas de hardware.

`Unrestricted` remove apenas headroom voluntário do AppCore. Kernel, driver,
firmware, controle térmico e limites elétricos continuam soberanos.

## Fit do modelo e admissão no device exato

Backends declaram componentes de pico de modelo, runtime/contexto e batch antes
da admissão:

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

O router chama admissão no device exato e combina carga e memória atuais com
as métricas do backend. O scheduler pode favorecer modelo residente e menor
custo de transferência/ativação, mas nunca ignora recusa de capacidade.
Capacidade necessária desconhecida falha fechada.

O mesmo budget limita `DynamicBatcher` com `BatchPressure::from_budget`, cria
tiers de residency sem sobreposição e faz treino readmitir antes de cada batch.
Treino reduz batch após admission limitada por pressão e para com segurança em
defer/reject, sem executar batch mínimo às cegas. Anúncios
Swarm são limitados novamente por `AiContributionPolicy`; capacidade local não
é doada implicitamente.

## Custo e motivo da feature NVIDIA

`accelerator-nvidia` adiciona opcionalmente `nvml-wrapper 0.12.1`, wrapper
seguro MIT/Apache que carrega a NVML do sistema dinamicamente. O grafo default
continua sem ela. O target adiciona `libloading`, `nvml-wrapper-sys`,
`bitflags`, `static_assertions`, `thiserror` e suporte de macros. A dependência
é necessária porque APIs padrão do SO não oferecem memória framebuffer e
utilização NVIDIA de forma portátil. AppCore usa apenas queries e degrada para
unknown/fallback sysfs quando a NVML não inicia. A feature não instala driver.

Interfaces primárias usadas:

- [contadores `/proc` Linux](https://docs.kernel.org/filesystems/proc.html)
- [sysfs AMDGPU](https://docs.kernel.org/6.12/gpu/amdgpu/thermal.html)
- [memória no Windows](https://learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex)
- [estatísticas Mach VM](https://developer.apple.com/documentation/kernel/vm_statistics64_data_t)
- [indicação de memória unificada Apple](https://developer.apple.com/documentation/metal/mtldevice/hasunifiedmemory)
- [queries NVIDIA NVML](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html)

## Operação e métricas

`HardwareSamplerMetrics` expõe `samples`, `sample_failures`, `cache_hits` e
`snapshot_age`. `ResourceGovernorMetrics` acrescenta `admission_denied`,
`device_count` e pressão de CPU/memória. O adapter do host pode mapear para
`resource.samples`, `resource.sample_failures`, `resource.snapshot_age`,
`resource.cpu_pressure`, `resource.memory_pressure`, `resource.device_count` e
`resource.admission_denied` no `appcore-ops`, sem usar DeviceId como label.

Para certificar produção, execute o relatório e o benchmark de modelo real em
cada classe de deploy. O exemplo OpenAI-compatible aceita
`APPCORE_AI_BENCH_ITERATIONS`; mede conclusão cold, throughput warm e snapshots,
mas não promete first-token latency porque o contrato atual não faz streaming.
