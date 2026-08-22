# Hardware resources, admission and placement

[Português](resources.pt.md) | [Français](resources.fr.md) |
[Guide](guide.en.md) | [Performance](benchmarks.en.md)

This page documents the production resource boundary delivered in
`appcore-ai 0.1.0-beta.1`. Detection informs policy; it never changes clocks,
fan curves, power limits, drivers or operating-system safeguards.

```text
machine capacity -> current availability -> AppCore mode budget
                 -> model/runtime/batch fit -> exact device placement
                 -> bounded batching, residency, training and contribution
```

## Run the hardware report

```bash
cargo run -p appcore-ai --example hardware_report
```

On Linux or Windows with the NVIDIA Management Library installed, opt into the
read-only NVIDIA probe:

```bash
cargo run -p appcore-ai --example hardware_report \
  --features accelerator-nvidia
```

The report prints only bounded aggregate capacity, load, topology, failure
classes, calculated mode budgets and sampler counters. It does not print host
identity, paths, driver error strings, prompts or secrets.

## Reading a real snapshot

```rust
use appcore_ai::{HardwareProbe, SystemHardwareProbe};

let probe = SystemHardwareProbe::default();
let snapshot = probe.sample()?;

println!("logical CPU: {:?}", snapshot.logical_cpus);
println!("available RAM: {:?}", snapshot.available_memory_bytes);
for device in &snapshot.devices {
    println!(
        "{}: {:?} {:?} free={:?} load={:?}",
        device.id,
        device.kind,
        device.capabilities.memory_kind,
        device.available_memory_bytes,
        device.utilization_percent,
    );
}
```

The default sampler is process-global, on-demand and cached for one second. It
has no polling thread, so idle CPU cost is zero. After the interval, one reader
performs the physical refresh while concurrent readers wait for that result.
Successful samples and failures are cached to prevent probe storms.

Use an independent interval only for a host diagnostic or a measured
deployment requirement:

```rust
use appcore_ai::SystemHardwareProbe;
use std::time::Duration;

let probe = SystemHardwareProbe::with_sampling_interval(
    Duration::from_millis(500),
)?;
let fresh = probe.refresh()?; // explicit diagnostic refresh
let metrics = probe.metrics();
```

`refresh` bypasses freshness, not single-flight. Do not call it on every
request. `captured_at_unix_ms` is wall-clock diagnostic time;
`ResourceGovernor` receives a caller-provided monotonic `now_ms`, and those two
time domains must not be mixed.

## Snapshot semantics

- `None` means unknown or unavailable. It never means zero and is never turned
  into unlimited capacity.
- `total_memory_bytes`, `available_memory_bytes` and `used_memory_bytes` follow
  the operating system's availability semantics. They are not process RSS.
- `process_cpu_percent` is normalized to whole-machine capacity; host CPU load
  is separate.
- Each accelerator has one stable probe-local `DeviceId`. A backend must bind
  that exact ID to its own runtime device and validate that the declared API is
  actually usable.
- `compatible_apis` indicates a driver/API family discovered by the probe; it
  is not proof that a particular model/backend combination can initialize.
- A disappeared or driver-lost device becomes unavailable and cannot receive a
  new placement.
- Failures contain only component and stable class: `Unavailable`,
  `PermissionDenied`, `Driver` or `InvalidData`.

### Dedicated versus unified memory

A discrete GPU has a dedicated VRAM pool. Fit is checked against the exact
target GPU; two 8 GiB GPUs are never treated as one 16 GiB GPU.

Apple Silicon exposes a unified memory pool. RAM and GPU allocations therefore
consume one budget, are checked once, and produce a `Memory` residency tier
instead of a fictitious second `Vram` tier. Unknown topology is kept unknown.

## Platform matrix

`Implemented` describes code present in this beta. `Tested here` means it was
executed on the reference Apple Silicon host; cross-compilation is not physical
certification.

| Platform | CPU topology/load | RAM | GPU/VRAM | Pressure/thermal | Beta evidence |
|---|---|---|---|---|---|
| macOS Apple Silicon | Implemented | Implemented, conservative free + inactive availability | Apple integrated GPU, unified memory, Metal family; utilization unavailable | unavailable | executed on Apple M1 |
| macOS Intel | Implemented | Implemented | generic GPU unavailable | unavailable | compiled, not physically tested |
| Linux | `/proc` + cached sysfs topology | `MemAvailable`; PSI memory when exposed | AMD DRM sysfs is partial; NVIDIA sysfs fallback is partial | memory PSI; thermal unavailable | cross-compiled, not physically tested |
| Linux + `accelerator-nvidia` | as above | as above | NVML total/free/used VRAM and utilization per exact NVIDIA device | thermal unavailable | compiled, no physical NVIDIA run |
| Windows | native Win32 CPU/process/system counters | `GlobalMemoryStatusEx` | NVIDIA only with optional NVML; generic GPU unavailable | unavailable | cross-compiled, not physically tested |
| other targets | logical CPU only when the standard library reports it | unavailable | unavailable | unavailable | explicit `Unsupported` snapshot |

NPU discovery is represented in the contracts but no trustworthy portable NPU
probe is delivered in this beta. It is reported as unavailable, never faked.

Implementation sources use bounded native reads: `/proc` and `/sys` on Linux,
Mach/sysctl on macOS, and Win32 system APIs on Windows. There is no shell
command, WMI subprocess, write-capable vendor call or periodic scanner.

## Modes and dynamic protection

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

| Mode | CPU/GPU ceiling | Capacity headroom | Concurrency intent |
|---|---:|---:|---|
| `Eco` | 40% | 30% | one job |
| `Balanced` | 70% | 20% | half of calculated workers |
| `Performance` | 90% | 10% | up to calculated workers |
| `Unrestricted` | 100% | 0% voluntary headroom | still bounded by configured maxima and OS/driver safety |
| `Custom` | caller CPU/RAM/VRAM/work/job ceilings | exact caller ceiling within detected availability | caller-bounded |

The default governor additionally reserves 256 MiB RAM, caps 64 workers and
eight concurrent jobs, and requires three consecutive pressure/recovery
samples before changing pressure state. Critical thermal state, high CPU/GPU,
memory PSI, low available RAM/VRAM, unhealthy devices, queue depth and active
jobs feed admission. Under stable pressure, percentage, memory, workers and
jobs are halved. These values are policy defaults, not hardware promises.

`Unrestricted` removes only voluntary AppCore headroom. The kernel, driver,
firmware, thermal control and electrical limits remain authoritative.

## Model fit and exact-device admission

Backends declare peak model, runtime/context and batch components before work
is admitted:

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

The router calls exact-device admission and overlays current hardware load and
available memory onto backend placement metrics. The scheduler can reward a
resident model and lower transfer/activation cost, but cannot override a
capacity denial. Unknown required capacity fails closed.

The same budget constrains `DynamicBatcher` through
`BatchPressure::from_budget`, produces only non-overlapping residency tiers,
and lets training re-admit before every next batch. Training shrinks after a
pressure-limited admission and stops safely on defer/reject instead of running
a blind minimum batch. Swarm advertisements
are clamped again to `AiContributionPolicy`; local capacity is never donated
implicitly.

## NVIDIA feature cost and rationale

`accelerator-nvidia` adds optional `nvml-wrapper 0.12.1`, a safe
MIT/Apache-licensed wrapper that loads the system NVML library dynamically.
The default graph remains free of it. The target tree adds `libloading`,
`nvml-wrapper-sys`, `bitflags`, `static_assertions`, `thiserror` and procedural
macro support. The dependency is justified because standard OS APIs do not
provide portable NVIDIA framebuffer-memory and utilization counters. AppCore
uses query methods only and degrades to unknown/sysfs fallback if NVML cannot
initialize. Enabling the feature does not install a driver.

Primary interfaces used by the implementation:

- [Linux `/proc` counters](https://docs.kernel.org/filesystems/proc.html)
- [Linux AMDGPU sysfs](https://docs.kernel.org/6.12/gpu/amdgpu/thermal.html)
- [Windows memory status](https://learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex)
- [Apple Mach VM statistics](https://developer.apple.com/documentation/kernel/vm_statistics64_data_t)
- [Apple unified-memory indication](https://developer.apple.com/documentation/metal/mtldevice/hasunifiedmemory)
- [NVIDIA NVML device queries](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html)

## Operations and metrics

`HardwareSamplerMetrics` exposes physical `samples`, `sample_failures`,
`cache_hits` and `snapshot_age`. `ResourceGovernorMetrics` additionally exposes
`admission_denied`, `device_count`, CPU pressure and memory pressure. A host
adapter should map them to low-cardinality `appcore-ops` names such as
`resource.samples`, `resource.sample_failures`, `resource.snapshot_age`,
`resource.cpu_pressure`, `resource.memory_pressure`, `resource.device_count`
and `resource.admission_denied`. Do not add device IDs as metric labels.

For production certification, run the hardware report and real-model benchmark
on every deployment class. The OpenAI-compatible example accepts
`APPCORE_AI_BENCH_ITERATIONS`; it records cold completion, warm throughput and
resource snapshots but cannot claim first-token latency because the current
contract is non-streaming.
