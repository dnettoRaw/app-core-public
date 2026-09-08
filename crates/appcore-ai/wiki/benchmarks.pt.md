# Laboratório de performance e relatório de otimização

[English](benchmarks.en.md) | [Français](benchmarks.fr.md) | [Guia](guide.pt.md)

Este relatório compara os mesmos workloads determinísticos do `perf_lab` antes
e depois do hardening alpha de 2026-08-22. Baseline é o run inicial; os valores
finais são a mediana de cinco runs release. A seção de recursos declara sua
baseline e protocolo separados. É evidência de engenharia, não garantia
portátil nem limite de CI.

## Reproduzir as medições

```bash
cargo bench -p appcore-ai --bench perf_lab --all-features
APPCORE_AI_BENCH_FORMAT=jsonl \
  cargo bench -p appcore-ai --bench perf_lab --all-features
cargo bench -p appcore-ai --bench alpha_harness --all-features
```

O JSON Lines contém workload, iterações, throughput, wall time e p50/p95/p99 em
nanosegundos. Os dados e limites são fixos, mas frequência de CPU e outros
processos não são controlados. Compare distribuições e repita no hardware do
deploy em vez de tratar um valor como SLO.
`artifact_memory_owned_1mib` e `artifact_memory_lease_1mib` são controles sobre
os mesmos bytes residentes verificados: o primeiro exige ownership exclusivo
em `Vec<u8>` e o segundo mede o lease compartilhado usado pelo Candle.

Host de referência: Apple M1 MacBookPro17,1, 16 GiB, Darwin arm64,
`rustc 1.97.1`, release. O processo final foi executado diretamente depois do
build para excluir a memória do compilador. Com o workload explícito de request
de 1 MiB, o macOS reportou 11,4 MiB de RSS máximo e 6,5 MiB de peak footprint.

## Antes e depois

| Workload | p50 inicial | p50 final | Mudança |
|---|---:|---:|---:|
| resolve lightweight com hit | 583 ns | 500 ns | -14,2% |
| rota de modelo ausente | 583 ns | 542 ns | -7,0% |
| backend quente, 1 rota | 2,250 us | 2,042 us | -9,2% |
| backend quente, 32 rotas | 96,417 us | 21,958 us | **-77,2%** |
| load frio de modelo único | 2,875 us | 2,625 us | -8,7% |
| scheduler, 32 candidatos | 4,834 us | 4,500 us | -6,9% |
| artefato local completo, 1 MiB | 3,409 ms | 3,086 ms | -9,5% |
| range local de 4 KiB | 16,583 us | 24,667 us | +48,7% |
| batch Candle, 1 item | 2,250 us | 2,375 us | +5,6% |
| batch Candle, 8 itens | 17,708 us | 18,708 us | +5,6% |
| batch Candle, 32 itens | 68,959 us | 31,041 us | **-55,0%** |
| scheduler Swarm, 1.000 peers | 226,958 us | 218,625 us | -3,7% |

As diferenças pequenas de batch Candle 1/8 são abaixo de um microssegundo mais
ruído e não foram escondidas; batch 32 mostra o crossover vetorizado. A
regressão de range é um custo de segurança intencional: toda leitura abre sem seguir
links e revalida o handle, o tipo de arquivo regular e o tamanho exato contra
substituição por symlink/reparse.

| Componente | 1 | 32 | 128 |
|---|---:|---:|---:|
| candidatos owned do model registry | 333 ns | 8,375 us | 28,667 us |
| leases compartilhados do model registry | 42 ns | 375 ns | 875 ns |
| cost scheduler | 125 ns | 4,500 us | 20,583 us |

O Swarm é medido diretamente com 1/10/100/1.000 peers; o JSONL traz os valores
exatos. O batching final teve p50 de 458 ns, 625 ns, 875 ns, 1,417 us e
2,542 us para 1/2/4/8/16 itens. O training Candle pequeno, com 64 exemplos e
duas épocas, teve p50 de 311,750 us. Validar por empréstimo um request binário
de 1 MiB mediu 42 ns contra 19,958 us do controle com clone explícito, cerca de
475 vezes; o controle demonstra a cópia removida, não é outro baseline histórico.

O run pareado do registry em 2026-09-02 usou 2.000 iterações por tamanho. Com
128 modelos, o lease compartilhado foi cerca de 33 vezes mais rápido que o
adapter owned compatível e reteve somente handles `Arc` no resultado. O router
de produção usa o lease e transfere as mesmas alocações às rotas locais.

O run release de verificação de 2026-09-02 para os novos controles pareados de
artefato mediu p50 de 25,333 us no load com ownership exclusivo e 42 ns no lease
compartilhado, diferença de cerca de 603 vezes no host de referência. Este run
único valida o workload e o custo esperado de ownership; repita as distribuições
antes de usá-lo em uma comparação de release.

A comparação calibrada de 2026-09-03 com `appcore-dev bench --name appcore-ai`
mediu a revalidação idempotente de um artefato local existente de 32 MiB. Cinco
amostras em processos mudaram p50 de 215,29 ms para 215,83 ms (+0,25%, dentro
da margem de ruído) e reduziram RSS pico de 65,86 MiB para 33,84 MiB (-48,61%);
os deltas de RSS da carga e retido caíram de 32,03 MiB para zero porque a
validação usa um buffer fixo de 16 KiB.

O mesmo laboratório agora inclui `openai_sse_coalesced_256x4kib`, que entrega
256 frames completos de texto com 4 KiB em um único chunk limitado. Em cinco
processos release, a mediana p50 caiu de 46,78 ms para 44,27 ms (-5,36%), a
mediana p95 de 47,35 ms para 45,09 ms (-4,76%) e o throughput mediano subiu
cerca de 5,3%. O caminho de produção analisa esses frames do chunk emprestado
com capacidade pendente zero; input fragmentado retém somente a cauda incompleta.

O workload em processo `model_registry_location_pressure_65536` exercita
65.536 localizações de peers distintas. Antes da admissão limitada, todas eram
retidas, com 35,995 ms p50 e 7,83 MiB de RSS pico/retido. Com o teto default por
modelo, 128 ficam retidas, as demais adições são rejeitadas antes de
copy-on-write e os contadores de pressão registram o resultado. Cinco processos
mediram 25,218 ms p50 (-29,94%), 1,91 MiB de RSS pico (-75,61%) e 1,89 MiB de
RSS retido (-75,86%). Este workload mede a fronteira de capacidade: comprova
retenção e rejeição limitadas, não trabalho aceito idêntico antes e depois.

O workload em processo `candle_model_load_4096x256` carrega um classificador
`NativeLinearV1` representativo de 4 MiB em um backend novo. Antes de mover os
vetores decodificados ao Candle, cinco processos release mediram 1,701 ms de
tempo mediano e 20,16 MiB de RSS pico mediano. A transferência de labels, pesos
e biases, seguida da reserva de contagem/bytes antes da leitura, mediu 1,368 ms
(-19,56%) e 16,11 MiB (-20,08%). O gauge de bytes contabiliza o tamanho exato
do artefato codificado; overhead de map/tensors continua limitado pelo teto de
modelos.

O workload em processo `lightweight_normalize_1mib_words` exercita o teto
default de 1 MiB com 524.288 palavras de um byte. O caminho antigo
collect-then-join retinha uma entrada `Vec<&str>` por palavra. Cinco processos
release mediram 4,823 ms de tempo mediano e 12,98 MiB de RSS pico mediano.
Escrever direto na `String` de saída mediu 3,337 ms (-30,80%) e 3,88 MiB
(-70,16%), preservando a semântica de whitespace Unicode.

O caminho de recursos de produção foi medido separadamente no mesmo Apple M1:

| Operação default-light do `alpha_harness` | p50 antes | Mediana p50 final | Mudança |
|---|---:|---:|---:|
| resolve lightweight | 1,542 us | 875 ns | -43,3% |
| snapshot compartilhado em cache | 541 ns | 167 ns | -69,1% |

| Operação de recurso | p50 final | p95 | Significado |
|---|---:|---:|---|
| snapshot compartilhado em cache | 167 ns | 208 ns | leitura normal de request/scheduler |
| amostra dinâmica forçada | 2,416 us | 3,208 us | refresh nativo CPU/RAM/device; somente diagnóstico |
| descoberta estática independente | 2,833 us | 3,834 us | setup novo de sampler/topologia |

Os valores finais de hot path são a mediana de cinco runs release consecutivos;
o valor anterior é o run registrado antes da mudança. A tabela dinâmica/estática
é a execução all-feature separada do `perf_lab`. O snapshot antigo não coletava
o detalhe atual de CPU/RAM/devices, portanto a melhora prova a fronteira de
cache, não leituras nativas mais rápidas. Amostragem física fica fora do hot path.
Um teste deixa o sampler ocioso sem leituras e observa zero chamadas ao probe;
o sampler não possui thread de polling nem buffer de histórico.

## Hotspots e alterações

A prioridade inicial foi construção de rotas, Candle item a item, I/O de
artefato, scans do scheduler e contenção. Agora o código:

- pré-calcula modalidades/bytes, faz lookup direto de backend forçado e mapeia
  rotas pontuadas sem scan O(n²);
- revalida partes emprestadas sem clonar texto/imagem/documento no início de
  cada `resolve`;
- compartilha records imutáveis de modelo com `Arc` entre rotas;
- executa um matmul e softmax Candle vetorizado em batches de até 64, mantendo
  validação e erro individual por item;
- executa o probe fora do mutex e colapsa refresh concorrente em uma amostra,
  inclusive armazenando falhas pelo intervalo limitado;
- mantém load de modelo single-flight com estado ready LRU limitado e métricas
  de load/wait/hit/eviction/invalidation;
- adapta batch por latência, pressão, memória e limite do backend;
- compartilha bytes imutáveis do artefato de memória por lease curto; somente o
  caminho compatível que exige ownership faz a cópia;
- limita registries, scheduler, residency, loads, peers, claims e transferências.

Não foi adicionado cache de resultado a `resolve`: outputs podem ser sensíveis,
não determinísticos ou dependentes do backend. O reuso fica restrito a samples
de recurso, artefatos verificados, loads prontos e metadados de residency, com
invalidação explícita ou capacidade fixa.

## Evidência de CPU, memória e concorrência

A suite final medida terminou em 1,14 s wall, 0,62 s user e 0,10 s system no
host de referência. São totais do processo, não budgets por request. O stress
executa 20.000 requests lightweight por default, até 1.000.000 via
`APPCORE_AI_SOAK_ITERATIONS`, e confirma telemetria e gauges vazios; a
certificação executou 100.000.

Testes concorrentes cobrem 100 requests dividindo um load frio, 32 writers de
artefato, cancelamento/deadline antes do dispatch, probe single-flight,
saturação de filas e churn de 1.000 peers. Fuzz cobre artefato nativo,
fronteiras de contrato e decoder OpenAI-compatible limitado.

Ownership lógico de memória também é limitado:

| Dono | Limite default ou fixo | Alocação |
|---|---|---|
| request/response | 1 MiB cada, 16 partes, 3 attempts | revalidação emprestada, sem deep clone em `resolve` |
| execution queue | 8 ativos + 128 esperando | erro de capacidade antes de crescer |
| batcher | 32 keys, 256 total, 64/key, 16/dispatch | backend pode reduzir o dispatch; Candle direto rejeita acima de 64 |
| registries/planners | 4.096 models, 256 backends, 4.096 rotas load/learned/resident e 256 reservations | maps fixos; ready loads usam LRU |
| artefato | máximo agregado escolhido pelo caller | `load_lease` compartilha bytes residentes e impede eviction enquanto ativo; `load` owned copia por contrato; range aloca só o range |
| Swarm | 4.096 peers; 64 devices e 1.024 artifacts/peer | metadata/transfer limitados; modelo fora do RPC genérico |
| Candle | batch inferência 64; training 512, 4.096 dimensões, 256 classes e artifact 64 MiB | dataset pode ser paged/file-backed, um exemplo limitado por vez |

Contagem de chamadas do allocator não foi instrumentada porque o host não tem
profiler no gate e a crate não instala um allocator global intrusivo. Memória
peak, controles de cópia de request/artefato e limites lógicos têm evidência;
profiling de allocations continua como evidência externa para RC/certificação.

## Limites da interpretação

O benchmark HTTP não inclui rede nem modelo real. Candle usa um classificador
linear pequeno, não um LLM. Cache do filesystem afeta artefatos. Startup
GPU/NPU, probes NVIDIA/AMD físicos, engines GGUF/MLX reais, tokens/s, energia,
temperatura e caudas de rede precisam ser medidos no deploy. O relatório de
hardware executou no Apple M1; Linux/Windows e a feature NVIDIA têm evidência
de compilação/testes determinísticos, não certificação física.
