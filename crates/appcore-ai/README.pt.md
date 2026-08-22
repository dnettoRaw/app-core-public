# appcore-ai

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md) |
[Exemplo básico](wiki/examples/basic.pt.md) |
[Exemplo Candle](wiki/examples/intermediate.pt.md) |
[Receitas](wiki/recipes.pt.md) |
[Modelos](wiki/models.pt.md) |
[LLMs generativos](wiki/generative-llm.pt.md) |
[Recursos de hardware](wiki/resources.pt.md) |
[Performance](wiki/benchmarks.pt.md)

Orquestração de IA limitada e independente de backend para o AppCore Runtime,
com SemVer independente. A versão atual é `0.1.0-beta.1`; ela não altera nenhum
manifest ou contrato wire V1 congelado do AppCore.

O build default oferece requests e responses validados, modalidades explícitas,
perfis de qualidade, caminho lightweight determinístico, governança de
hardware/recursos, scheduler orientado a custo, filas justas e batching
limitados, load single-flight por modelo/backend, registries de
modelos/artefatos, residency em tiers, fronteiras de provenance, telemetria redigida e a API assíncrona
`AiRuntime::resolve`. Ele não possui dependência de framework de ML.

A release beta também oferece batching adaptativo conforme backend, batch
Candle vetorizado, coordenação LRU limitada de load e `ModelLoadSnapshot`
público. Artefatos locais usam abertura no-follow, revalidação do handle e
ativação atômica sem substituição. Registries, rotas aprendidas, residency,
loads e claims Swarm têm limites fixos.

Features opcionais são explícitas:

- `accelerator-nvidia`: detecção read-only de VRAM/utilização NVIDIA por NVML
  carregada dinamicamente no Linux/Windows; ausente do grafo default;
- `backend-candle`: inferência CPU real para modelos limitados `NativeLinearV1`;
- `backend-openai-compatible`: transporte chat-completions real e limitado para
  llama.cpp, MLX-LM, TabbyAPI, vLLM, SGLang, TensorRT-LLM, OpenVINO ou servidor
  compatível testado explicitamente;
- `training-candle`: SGD local reprodutível, checkpoints atômicos e resume;
- `swarm`: contratos experimentais de bridge autenticada, peers expirantes,
  contribuição separada de compute/storage e failover.

O contrato generativo inclui chat com papéis, sampling limitado, ferramentas e
tool calls tipados e imagens. O adapter HTTP executa texto/chat e, quando o
servidor/modelo declara a capacidade, análise de imagem. PDF é uma modalidade
de primeira classe, mas ainda exige backend de documentos escolhido pela
aplicação; o core não embute parser PDF/OCR universal inseguro.
`SegmentedModelReader` faz leitura por range com digest por segmento sem afirmar
que todo engine suporta expert streaming.

Swarm nunca cria outro control plane ou sistema de autenticação. Um adapter do
host deve usar segurança, capabilities e Peer RPC do AppCore. Compute remoto
exige grants explícitos por tenant, e bytes vindos de peers são verificados
antes da ativação.

```bash
cargo test -p appcore-ai
cargo test -p appcore-ai --all-targets --all-features
./crates/appcore-ai/scripts/check-feature-matrix.sh
cargo test -p appcore-ai --test stress_soak --all-features
APPCORE_AI_BENCH_FORMAT=jsonl cargo bench -p appcore-ai --bench perf_lab --all-features
```

`Unrestricted` remove somente o headroom voluntário do AppCore. Ele não pode
desligar proteções de SO, driver, firmware, temperatura ou energia, nem garante
que o hardware não sofrerá throttling.

Exemplos executáveis:

```bash
cargo run -p appcore-ai --example lightweight_runtime
cargo run -p appcore-ai --example hardware_report
cargo run -p appcore-ai --example candle_runtime --features backend-candle
cargo run -p appcore-ai --example openai_compatible --features backend-openai-compatible
cargo run -p appcore-ai --example candle_training --features training-candle
```

A feature deliberadamente experimental `appcore-bin/ai-alpha` entrega um fluxo
explícito pelo Supervisor e `CapabilityRegistry` sem alterar manifests V1. A
seleção declarativa permanece trabalho pós-1.0 e não faz parte da claim beta.
Consulte o
[relatório de release](wiki/release-readiness.pt.md) e o
[threat model](wiki/threat-model.pt.md).
