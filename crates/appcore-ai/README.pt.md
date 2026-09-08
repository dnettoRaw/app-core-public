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
com SemVer independente. A release atual é `0.1.0-beta.3`; ela não altera
nenhum manifest ou contrato wire V1 estavel do AppCore.

O build default oferece requests e responses validados, modalidades explícitas,
perfis de qualidade, caminho lightweight determinístico, governança de
hardware/recursos, scheduler orientado a custo, filas justas e batching
limitados, load single-flight por modelo/backend, registries de
modelos/artefatos, residency em tiers, fronteiras de provenance, telemetria redigida e a API assíncrona
`AiRuntime::resolve`. Ele não possui dependência de framework de ML.
A normalização lightweight de whitespace Unicode constrói somente sua `String`
de saída limitada, sem reter uma lista intermediária de todas as palavras.

A release beta também oferece batching adaptativo conforme backend, batch
Candle vetorizado, coordenação LRU limitada de load e `ModelLoadSnapshot`
público. Artefatos locais usam abertura no-follow, revalidação do handle e
ativação atômica sem substituição. Registries, rotas aprendidas, residency,
loads e claims Swarm têm limites fixos.
Uma ativação local idempotente ou corrida entre writers revalida e compara o
artefato existente incrementalmente com um buffer fixo de 16 KiB; nunca carrega
um segundo artefato completo ao lado dos bytes do caller.
`ArtifactStore::load_lease` permite ao backend usar bytes verificados durante o
decode: tiers de memória compartilham o `Arc<[u8]>` residente, enquanto arquivo
e peer preservam sua alocação já owned. Leases de memória ativos continuam na
contabilidade do store e impedem eviction até serem liberados.
`ModelRegistry::get_lease` e `candidate_leases` também devolvem snapshots
imutáveis compartilhados dos modelos. O router usa esses leases diretamente,
portanto a descoberta de rotas não clona mais descriptors completos para
depois cloná-los novamente nas rotas locais. `get` e `candidates` continuam
como adapters owned compatíveis; mutações usam copy-on-write e nunca mantêm o
lock do registry durante trabalho do backend.
`ModelRegistryLimits` também limita modelos, localizações por modelo,
localizações totais e bytes contabilizados das localizações. Os tetos default
são 4.096 modelos, 128 localizações por modelo, 65.536 localizações e 8 MiB de
metadata de localização; callers podem escolher valores menores. Iteradores
iniciais e adições posteriores falham antes da retenção, duplicatas continuam
idempotentes sem copy-on-write e `ModelRegistry::pressure` expõe contagem e
bytes atuais/de pico, além das rejeições.
O loader Candle transfere labels, pesos e biases decodificados ao modelo
carregado sem clonar esses buffers completos. `CandleBackend` reserva um slot e
os bytes declarados do artefato antes da leitura ou decode;
`new_with_loaded_byte_limit` reduz o teto agregado e `memory_pressure` expõe
uso atual/de pico e rejeições antecipadas. A reserva acompanha leases de
inferência ativos depois de `unload`. Esse backend é classificador e não possui
KV cache generativo; engines generativas externas devem limitar o próprio cache.
Bodies codificados OpenAI-compatible usam a mesma alocação imutável
compartilhada no request de transporte, na passagem ao worker blocking e no
`HttpRequest` de baixo nível. Isso remove duas possíveis cópias integrais sem
alterar o trait emprestado de transporte nem o cancelamento limitado.

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

Detectar GPU não é executar inferência GPU. Candle continua CPU-only mesmo com
`accelerator-nvidia`; ambos os adapters rejeitam IDs de dispositivo não
registrados, e o adapter HTTP faz isso antes de codificar ou enviar. O vínculo
com dispositivo físico externo pertence ao deployment, não ao protocolo chat.
Veja a [matriz de execução](wiki/resources.pt.md#matriz-de-execução-detecção-não-é-inferência).

O contrato generativo inclui chat com papéis, sampling limitado, ferramentas e
tool calls tipados e imagens. O adapter HTTP executa texto/chat e, quando o
servidor/modelo declara a capacidade, análise de imagem. PDF é uma modalidade
de primeira classe, mas ainda exige backend de documentos escolhido pela
aplicação; o core não embute parser PDF/OCR universal inseguro.
`SegmentedModelReader` faz leitura por range com digest por segmento sem afirmar
que todo engine suporta expert streaming.

Esta release fortalece a fronteira OpenAI-compatible com status HTTP tipado e
`Retry-After` limitado, argumentos brutos de tool call recuperáveis, futures de
transporte realmente assíncronas, profiles de compatibilidade validados, output
JSON Schema opt-in e streaming cancelável com backpressure síncrono. Streaming
só existe quando capability e transporte do deployment declaram suporte; o
cliente HTTP bloqueante default é retirado da thread do executor e não finge
entrega incremental de rede.
O decoder SSE analisa frames completos coalescidos diretamente do chunk
emprestado, retém somente uma cauda incompleta e compacta bytes pendentes uma
vez por chunk.

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

Integração de deployment pode compor um fluxo explícito pelo Supervisor e
`CapabilityRegistry` sem alterar manifests V1. A
seleção declarativa permanece trabalho pós-1.0 e não faz parte da claim beta.
Consulte o
[relatório de release](wiki/release-readiness.pt.md) e o
[threat model](wiki/threat-model.pt.md).

## Documentação estável

ID estável: **ACR-022**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-022). Esse ID permanente
continua válido se a página da wiki mudar.
