# appcore-ui

Release candidate **`appcore-ui 1.0.0-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.

Módulo de UI independente, release candidate `1.0.0-rc.1`.

O módulo deve renderizar páginas nativas definidas em Rust e páginas web em
HTML/TypeScript. Ciclo de vida das janelas, renderização, ações, estado visual e
bridge com o Runtime pertencem ao appcore-ui. Layouts e schemas de negócio
pertencem à aplicação. Deployment seleciona explicitamente o backend de plataforma,
fornece autenticação e possui arranque/shutdown; o SDK não inicia um host de UI.

A implementação atual fornece contratos UI V1 limitados para commands, queries
e eventos, capabilities, ownership autenticado da superfície, cancelamento,
deadlines, backpressure e cliente TypeScript tipado. Commands preservam idempotência
e emissão de eventos pelo Runtime. Estes testes da bridge não provam renderização.

Os previews nativo e web local abaixo renderizam em macOS. A bridge Runtime pode ser ligada explicitamente.
A tela externa foi verificada num perfil sintético e isolado. Outras plataformas são trabalho futuro. Preferir recursos do sistema operativo, reutilizar
contratos existentes e justificar cada dependência externa. O módulo não exige
um shell de aplicação externo nem um framework de renderização empacotado.

UI V1 é independente das versões de manifests e protocolo peer. Entradas não
suportadas falham com `NO MORE SUPPORTED PLEASE UPDATE`. Notificações de UI são
limitadas e efémeras; reconstruir estado por query após reconexão ou perda
observável. Cancelamento não reverte um efeito já produzido.

## Preview nativo em fonte

`UiNativePageV1` declara controlos nativos em Rust. `UiNativePage::new`
(ou `decode`) valida versão 1, IDs únicos, geometria finita a partir do canto
superior esquerdo, 512 controlos, 256 KiB codificados, 4096 bytes por label e
até 4.194.304 píxeis lógicos de viewport. Este perfil inicial usa frames fixos;
ainda não é um sistema completo de layout responsivo. Debug não expõe conteúdo.

Em macOS, a feature explícita `native-macos` fornece `UiNativeApplication` e
`UiNativeWindow`. Deployment cria um controller por processo com orçamento de
janelas físicas (1–8), abre páginas validadas, processa lotes limitados (primeira espera até um segundo; depois até 64 eventos
com orçamento cooperativo de 2 ms) e consome ativações nativas. A fila por janela
admite 1–256 ativações. Overflow e panics capturados no hook de close têm
contadores. As ativações identificam bindings da aplicação; não dispensam a
autorização do Runtime nem executam commands por si. Close chama o hook uma
vez; Drop liberta janelas e controlos retidos. Uma janela fechada mantém a sua
vaga física até o handle ser descartado.

O backend usa três bindings diretos opcionais do sistema (`objc2`,
`objc2-app-kit`, `objc2-foundation`), cinco pacotes ativos incluindo transitivos. São
chamadas tipadas às APIs do SO, sem outro framework de renderização. Unsafe fica
isolado nas fronteiras documentadas de FFI/callback. A bridge e o modelo de página
por defeito não exigem estes pacotes. Outras plataformas nativas continuam trabalho futuro.

Executar o preview em fonte no macOS:

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

O preview tem duração limitada, mostra texto/botão nativos, emite apenas IDs de
controlos/ações e fecha normalmente. A janela macOS foi inspecionada visualmente;
o botão entregou `state.inspect` e o encerramento terminou com código 0. Esta
prova de renderização é distinta da qualificação do consumidor abaixo.

## Recursos web locais

`UiWebBundle` recebe HTML, JavaScript compilado, CSS, PNG/JPEG e WOFF2 imutáveis
produzidos pelo build da aplicação. Não acede a ficheiros ou à rede. Paths são
identificadores relativos ASCII explícitos; traversal, encoding, URLs e query
strings são rejeitados. Lookup não possui fallback. Há limites de 512 assets,
2 MiB por asset e 16 MiB retidos no total, MIME explícito e validação UTF-8 dos
textos. Estes limites abrangem bytes de fonte retidos, não imagens/fontes
descodificadas nem memória do engine do sistema.

A política inicial nega ligações remotas, scripts/styles inline, frames, forms e
plugins. O preview abaixo aplica CSP e nega navegação externa;
o modelo de dados sozinho não renderiza páginas web. A qualificação abaixo verifica uma tela externa num consumidor isolado. Esta
fronteira de recursos não acrescentou dependências.

## Preview web direto em fonte

Em macOS, a feature explícita `web-macos` fornece `UiWebWindow`, usando o
componente web do sistema, um binding opcional de plataforma (`objc2-web-kit`)
e o binding de callback blocks (`block2`). Não empacota browser ou outro shell
de aplicação. O crate por defeito continua sem estas dependências de plataforma.

O host serve apenas recursos do bundle numa origem local aleatória por janela.
Emite CSP e nosniff, rejeita navegação externa/popups/frames, credenciais, ports,
query e encoding em URLs, e usa perfil de dados não persistente. Admite 16 tasks
de recursos, observando cancelamento entre callbacks de resposta. Perda do engine
ou carregamento inicial acima de 30 segundos fecha a janela quando o deployment
continua a processar eventos. Não há retry automático ou host oculto. Estes
limites não abrangem toda a memória do engine do sistema.

```sh
cargo run -p appcore-ui --features web-macos --example web_page
```

A janela real carregou HTML/CSS/JavaScript, o botão atualizou o documento, uma
navegação externa foi rejeitada e close deixou zero janelas retidas. O preview
exercita ações DOM locais. A bridge descrita abaixo pode ser ligada
antes do carregamento; a tela existente foi qualificada num perfil isolado.

## Bridge Runtime no host web

Antes do primeiro pump, chamar `window.attach_surface(factory)`. A factory recebe
ID/origem reais da janela e devolve uma superfície com essa vinculação exata,
catálogo declarado e autorização obrigatória do deployment. O host verifica URL
do documento/main frame e schema; JSON da página não escolhe caller ou principal.
Documentos com bridge não navegam/recarregam implicitamente noutra geração.
Reconexão cria nova janela e superfície.

Replies Promise ficam numa fila limitada: orçamento de operações da superfície,
duas vagas reservadas para close/cancel e 16 MiB de pedidos. O pump explícito
processa uma passagem por tick. Autorização e backend devem ser não bloqueantes,
ou o deployment deve isolar trabalho bloqueante com limites. Não inicia executor
ou threads. Close rejeita Promises pendentes, cancela a superfície e liberta
subscrições; não reverte efeitos já realizados.

O cliente TypeScript exporta `connectUiWeb`, usando metadata pública do host e
transporte Promise nativo. Mantém operações tipadas, erros controlados, deadlines,
backpressure e chaves de idempotência. IDs usam a API de bytes aleatórios
criptográficos do sistema, sem biblioteca adicional.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

Esta fixture SDK explicitamente identificada usa o cliente TypeScript real no
componente web do sistema. Query, command, subscrição e replay com a mesma chave
foram verificados: um efeito, um evento Runtime e uma notificação UI. Autoridade
forjada e ID de outra superfície foram rejeitados. Close deixou zero pedidos
pendentes e janelas retidas. A autorização da fixture não é login de produção.
Um consumidor externo independente reutilizou a tela de estado da sincronização
e o CSS existentes. A janela real macOS apresentou estado sintético, realizou
duas queries pela bridge e terminou com código 0, zero pedidos pendentes e zero
janelas retidas. Isto valida um consumidor de fonte, sem dados/autenticação de
produção ou migração completa da aplicação. Gates gerais no plano de implementação.

## Exemplos interativos e funções nativas

O perfil nativo inclui agora texto editável, checkbox, dropdown, slider e progresso determinado, além de texto/botões. `elements::{text,button,input,toggle,select,slider,progress}` são construtores Rust puros; valide a página completa. `UiNativeWindow::{set_label,set_value,set_enabled,value}` atualizam/leem controlos existentes com validação tipada e limites de página/label, sem alterar ações ou conceder autorização. Dropdowns admitem 1–64 labels únicos não vazios; o input é limitado ao ler, sem garantir toda a memória dos controlos de edição do SO.

A galeria usa Editing → Review → Applied, revisões, campos bloqueados e comandos tipados por script. Não há IA ligada. O runner tem física determinística, colisão/score/restart/pausa. O exemplo 3D usa WebGL direto do sistema, perspetiva/profundidade e seis trajetórias de Lorenz, integração RK4, 24.576 pontos limitados, órbita/zoom, isolamento de trajetória, pausa e avanço por passos; não é engine 3D nativa Rust.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

Os quatro exemplos foram renderizados e usados em janelas macOS reais; fechar cada um terminou 0 com zero janelas retidas. A proposta de componentes/IA é roadmap, não protocolo de tools implementado.

## Resposta à interação

O pump serve o run loop mesmo sob carga de input e a bridge antes/depois do dispatch nativo. A drenagem para quando a fila de ativações enche; consuma `pending_activations` com `next_activation` depois do pump. Tracking/callbacks do sistema podem exceder o orçamento cooperativo. A WebView aceita o primeiro clique físico ao ativar uma janela inativa; muda o foco, não a autorização. Os testes físicos receberam dez de dez ações nos exemplos nativo e web, e um clique ao alternar janelas.

```toml
[dependencies]
appcore-ui = { version = "=1.0.0-rc.1", features = ["web-macos"] }
```

[crates.io](https://crates.io/crates/appcore-ui/0.1.0-beta.2) · [API](https://docs.rs/appcore-ui/0.1.0-beta.2/appcore_ui/)

## Candidato de release — 2026-10-14

`0.1.0-beta.2`: alinhamento de dependências.

A galeria 3D em fonte inclui também uma galáxia determinística de 12.000 estrelas, terreno procedural e uma superfície ondulada no vertex shader (8.192 triângulos cada). Seleção de cena, pausa/passo/reset e órbita/zoom usam o renderer WebGL limitado. São capacidades dos exemplos locais, não compute GPU nativo Rust, fluidos físicos ou uma promessa de desempenho 4K.

A galeria em fonte começa na Cidade fractal: 391 caixas (4.692 triângulos), três níveis de torres recursivas, pontes elevadas, iluminação direcional por face e nevoeiro. Clica no canvas para focar; WASD voa, Q/E muda altitude, Shift acelera, arrastar olha à volta, R regressa ao início e Esc liberta o foco. O voo tem coordenadas limitadas e não tem colisão sólida. As teclas são limpas na perda de foco/visibilidade/contexto. Este exemplo recursivo finito usa WebGL; 3D nativo Rust continua planeado.

Correção dos shaders: uniforms e varyings partilhados usam precisão explícita igual nos dois estágios WebGL; falhas de compilação/ligação mostram diagnóstico limitado do driver. Um teste de regressão verifica a interface.

Qualificação macOS após a correção: cidade renderizada, avanço W, altitude E, retorno R e troca para ondas animadas foram exercitados na janela web real do sistema. Isto não qualifica todas as cenas nem recuperação de perda do dispositivo.

Protótipo nativo em fonte: `gpu-macos` liga opcionalmente uma superfície Metal emprestada à janela. Uma tentativa por janela física, renderer na thread principal, até duas submissões, callbacks de conclusão, suspensão com dimensão zero e backpressure no resize. `native_gpu` possui pump de arranque/render/shutdown e apresenta um clear animado. Cena interativa/compute ainda pendentes. Perda de superfície exige reconfiguração explícita; perda do dispositivo encerra esta geração e deployment possui recriação. Aquisição/configuração da plataforma ainda pode esperar; não se afirma latência estrita.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_gpu
```

Qualificação de apresentação: reservas RAII passaram testes de libertação por falha/drop e concorrência entre 16 threads. Apresentação macOS real recebeu 10 cliques físicos mais 10 ativações nativas (contador 20), continuou após ampliar/restaurar e encerrou com frames=7622, clicks=20, retained_windows=0. Janelas minimizadas agora reportam dimensões gráficas zero para suspender submissões; minimizar/restaurar, perda injetada e recriação ainda precisam de qualificação.

Qualificação de recuperação passou em Metal real: seis ciclos minimizar/restaurar, duas perdas de estado de superfície injetadas, uma destruição explícita de dispositivo com callback de perda e recriação numa nova janela física. Gerações apresentaram 202 e 213 frames, ambas com pending_before_drop=0 e retained_windows=0. É destruição deliberada, não reset físico da GPU ou perda espontânea do compositor. Hooks exigem features opt-in `qualification` / `gpu-qualification`; defaults preservados.

```sh
cargo run -p appcore-ui --features gpu-qualification --example native_gpu_recovery
```

Lorenz nativo ligado à apresentação: RK4 GPU e quads instanciados, câmara em perspetiva, profundidade, seis cores e 24.576 amostras limitadas. Controlos nativos pausa/retoma, passo, reset, órbita/inclinação, zoom e isolamento. Verificação macOS real observou atrator completo, pausa, isolamento, câmara, reset e um passo aumentando amostras 240→288 (8×6). Retoma e shutdown passaram: frames=6747, actions=9, retained_windows=0. Sem readback de partículas por frame; admissão cobre compute e desenho. Warmup avança oito passos por frame apresentado. ECS/mundo/virtualização/física massiva continuam etapas futuras.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_lorenz
```

`GpuMeshBatch` oferece geometria estática opaca partilhada, poses/cores, frustum GPU conservador e draw_indexed_indirect. `GpuMeshScene` compõe até 64 batches fixos e 65536 instâncias reservadas: prepass frustum → Hi-Z atual por máximo → consultas conservadoras → índices compactados → desenho indireto final. Buffers e profundidades prepass/final são distintos; a pirâmide é reconstruída uma vez por frame, com padding distante em viewports ímpares. Casos cortados pela câmara/near-plane ou viewport permanecem visíveis. Grupos seguem ordem/IDs dos descritores; grupos vazios zeram contagens. Reserva gerida inclui duas profundidades, buffers e mips; a cena standalone exclui cor do chamador. Apresentação nativa admite também essa cor. A implementação inicial duplica geometria entre prepass/final, contabilizada no budget.

Testes de pixels Metal reais verificam profundidade/cor, exclusão/reaparecimento por máscara, composição multi-mesh, grupos inválidos, budget e limpeza de frame vazio. Qualificação nativa apresentou 256 cubos + 64 pirâmides, recebeu dez cliques físicos consecutivos na órbita e exercitou pausa, zoom, órbita, reset, retoma e ampliar/restaurar a janela. Shutdown: frames=3102, actions=15, retained_windows=0. Qualificação anterior cobriu seis ciclos minimizar/restaurar, duas perdas de superfície injetadas, destruição deliberada do dispositivo/callback e recriação explícita (202/213 frames, zero pending/owners). Isto não é reset físico da GPU. Hooks de falha exigem qualification/gpu-qualification.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_meshes
```

LorenzParameters expõe sigma (1–20), rho (1–50), beta (0.5–6), rejeitando valores não finitos. reset_with_parameters/configure_lorenz reiniciam trajetórias e histórico após conclusão; reset preserva coeficientes. O timestep permanece 0.005; limites não garantem estabilidade numérica universal. Testes Metal comparam coeficientes personalizados com referência f64 e verificam rejeição sem alteração. O botão nativo Params percorre três presets limitados. Qualificação na janela real exercitou 0→1→2→0, histórico zerado em pausa, passo GPU com 48 amostras, retoma e encerramento.

A feature opcional native-input-timing de appcore-ui acrescenta tempos locais sem alterar UiNativeActivation ou UI V1. next_timed_activation retorna tempo desde callback e, apenas durante dispatch mouse/teclado correlacionado pelo pump, tempo desde evento. Callbacks de acessibilidade sem correlação permanecem None. O contexto é restaurado após dispatch aninhado/unwind; relógios inválidos não produzem zero fictício. Tempo desde mouse-down inclui segurar/tracking até soltar. Uptime bruto nunca é serializado nem registado.

NativeRenderer.frame_progress reporta IDs submetidos/concluídos locais à geração via callbacks da fila; contadores não recuam nem dão wrap. Conclusão é trabalho GPU observado por polling, não apresentação do compositor nem scanout físico. A sonda native_meshes retém até 256 amostras/marcadores, reporta overflow/pendências e associa ações ao primeiro frame submetido depois delas; estados intermédios podem ser agregados. Retorno de present e conclusão observada são endpoints distintos. O perfil opcional utiliza systemUptime para tempo decorrido local; incluir examples/native-gpu/PrivacyInfo.xcprivacy (razão 35F9.1) no bundle da aplicação conforme [as razões de API da Apple](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype).

Qualificação: Metal, Apple M1, 1024×768 físicos, 320 instâncias/2 malhas, pump wait 8 ms. Dez cliques por coordenadas no percurso de eventos nativos e dez ativações de acessibilidade entregaram 20 ações. p50/p95/p99 desde evento nativo: ação 1.549/19.964/19.964 ms; retorno de present 13.326/30.874/30.874 ms; conclusão GPU observada 23.511/42.509/42.509 ms (n=10). Callback→ação agregado 2.289/103.954/104.105 ms (n=20); acessibilidade ficou sem timestamps inventados (uncorrelated=10). Overflow/pendências zero; submitted=completed=3435, retained_windows=0. A amostra pequena neste perfil não qualifica 4K60 nem latência universal.

```sh
cargo run -p appcore-ui --features gpu-macos,native-input-timing --example native_meshes
```

GpuMeshScene::new_lod agora compõe batches LOD com buffers de seleção separados para prepass/final, pirâmide Hi-Z atual partilhada e bounds de oclusão conservadores para todos os níveis. encode_lod recebe LodSceneFrame com matriz/view/grupos coerentes explícitos; usar encode estático numa cena LOD é rejeitado antes de writes. Geometria continua duplicada por passe e contabilizada. set_lod_resident altera disponibilidade nos dois passes de forma coerente, sem evicção. Construção/encode estáticos mantêm o contrato. NativeRenderer expõe enable_mesh_lods, present_mesh_lods e alterações de disponibilidade com frames drenados, preservando ownership da superfície, reservas de frames, admissão de pico no resize e conclusão GPU.

native_lod acrescenta 256 esferas refinadas/proxies octaédricos e 64 caixas/proxies tetraédricos, órbita/zoom/pausa, budgets 4/12/32/96 pixels e Fine on/off lógico. Reducer puro rejeita comandos desconhecidos; alinhamento câmara/projeção e controlos limitados têm testes. Metal real 512×512 com dois grupos LOD verifica perda/restauro de detalhe, reset vazio, admissão e modo explícito da cena. Processo nativo executou até deadline de 180 segundos: frames=3239, submitted=completed=3239, retained_windows=0. Esta execução teve actions=0. Qualificação visual/input/resize deste novo exemplo permanece pendente: automação nativa devolveu cgWindowNotFound nesta app e na app de controlo previamente qualificada. Não qualifica 4K60, streaming, compilação meshlet ou evicção de memória.

```sh
cargo run -p appcore-ui --features gpu-macos,native-input-timing --example native_lod
cargo test -p appcore-render --features metal --test lod_scene -- --include-ignored
```

O encerramento nativo expõe abort_close para erro ou expiração do prazo de drenagem. Fecha e destrói essa geração do dispositivo sem espera GPU nem atualização artificial de conclusão de frames; chamadas repetidas são inofensivas. Drop também destrói o dispositivo antes de libertar o owner emprestado da superfície, incluindo saídas antecipadas por erro. Para drenagem normal, usar begin_close/finish_close. A destruição é uma chamada ao backend, sem garantia absoluta de duração; abortar não prova execução ou apresentação dos frames pendentes. Após abortar, finish_close devolve RendererClosed mesmo depois de repetir begin_close; trabalho abortado não é reportado como drenagem normal concluída.

A admissão do resize Lorenz conta agora o depth Depth32Float antigo ainda retido, os novos alvos color/depth e os buffers fixos da simulação/câmara antes de reconfigurar a superfície. Dimensões iguais reutilizam o depth. Uma rejeição do pico preserva a cena e configuração anteriores; dimensões zero continuam a suspender. Testes puros cobrem crescimento/redução, limites exatos e dimensões inalteradas. A qualificação nativa rejeita também um crescimento cujo estado final cabe mas o pico de substituição excede o budget, apresentando depois a cena Lorenz preservada.

A feature opcional gpu-qualification acrescenta UiNativeWindow::qualification_content_size(width, height), em pontos lógicos. Altera o tamanho real do conteúdo AppKit dentro dos mínimos/máximos originais da página e do teto de área de 4194304 pontos, rejeitando tamanhos inválidos antes de mutar e janelas fechadas como indisponíveis. Não redimensiona o renderer: o owner executa o pump, lê graphics_size em pixels físicos e reconfigura explicitamente após conclusão GPU. native_gpu_recovery exercita agora nove resizes reais da janela Lorenz, três ciclos Lorenz de minimizar/restaurar, preservação do histórico em pausa, reset e um passo, com 21 frames submetidos/concluídos e zero janelas retidas. Pedidos de tamanho inválidos preservam o tamanho físico. Esta qualificação automática do ciclo de vida não prova qualidade visual nem latência de input físico.


Os exemplos native_lorenz, native_meshes e native_lod incluem HUD legível: presents bem-sucedidos/segundo, intervalo médio/p95 entre presents (128 amostras), tempo CPU da chamada present, RAM residente do processo (RSS), payload GPU gerido, resolução física, adaptador e contadores de frames submetidos/concluídos. Atualização duas vezes por segundo. CPU submit não mede execução GPU; GPU payload é contabilização de budget, excluindo driver/compositor/buffers adicionais da swapchain, não VRAM medida. Estas métricas não provam scanout nem 4K60.

Clicar abaixo do HUD ativa a câmara: arrastar roda, W/S ou roda do rato aproxima/afasta, A/D ou setas esquerda/direita rodam e setas cima/baixo inclinam. Escape, Tab, clicar nos controlos ou perder foco libertam a navegação e limpam teclas premidas. Atalhos do sistema passam normalmente. É órbita/zoom limitado, não voo livre nem manipulação de objetos. Navegação manual interrompe a órbita automática das malhas; Resume retoma. A simulação Lorenz é independente. As janelas ficam abertas até serem fechadas; E3DN_DEMO_SECONDS=30 (1–600) permite execução temporizada.

Com gpu-macos, enable_scene_input(overlay_height) ativa navegação de estado fixo numa janela gráfica; take_scene_input(elapsed) consome deltas, limita elapsed a 50 ms e informa foco sem expor texto bruto das teclas. native_process_resident_bytes lê RSS atual pela API Mach do sistema; falhas são explícitas. Labels sobre gráficos recebem fundo opaco para leitura.
