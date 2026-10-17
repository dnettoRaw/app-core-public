# Página web — fluxo Runtime

Compile o cliente tipado e execute a fixture SDK com catálogo de capabilities e autorização explícita da superfície. Executa uma query, um command idempotente e uma subscrição. Repetir a chave mantém um efeito e um evento Runtime. Close rejeita pedidos pendentes e liberta a superfície.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

[Rust](../../examples/web_bridge.rs) · [SDK](../../examples/web-bridge/runtime.rs) · [Client](../../examples/web-bridge/page.js) · [Guide](../guide.pt.md)

A sessão da fixture serve apenas para qualificação. Um deployment real deve autorizar o seu principal, tenant e capabilities e isolar providers bloqueantes. Não inicia executor escondido.

Estes exemplos estão incluídos no pacote beta público para macOS. Renderers de outras plataformas são trabalho futuro.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

A galeria usa Editing → Review → Applied, revisões, campos bloqueados e comandos tipados por script. Não há IA ligada. O runner tem física determinística, colisão/score/restart/pausa. O exemplo 3D usa WebGL direto do sistema, perspetiva/profundidade e seis trajetórias de Lorenz, integração RK4, 24.576 pontos limitados, órbita/zoom, isolamento de trajetória, pausa e avanço por passos; não é engine 3D nativa Rust.

A galeria 3D em fonte inclui também uma galáxia determinística de 12.000 estrelas, terreno procedural e uma superfície ondulada no vertex shader (8.192 triângulos cada). Seleção de cena, pausa/passo/reset e órbita/zoom usam o renderer WebGL limitado. São capacidades dos exemplos locais, não compute GPU nativo Rust, fluidos físicos ou uma promessa de desempenho 4K.

A galeria em fonte começa na Cidade fractal: 391 caixas (4.692 triângulos), três níveis de torres recursivas, pontes elevadas, iluminação direcional por face e nevoeiro. Clica no canvas para focar; WASD voa, Q/E muda altitude, Shift acelera, arrastar olha à volta, R regressa ao início e Esc liberta o foco. O voo tem coordenadas limitadas e não tem colisão sólida. As teclas são limpas na perda de foco/visibilidade/contexto. Este exemplo recursivo finito usa WebGL; 3D nativo Rust continua planeado.

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


Os exemplos native_lorenz, native_meshes e native_lod incluem HUD legível: presents bem-sucedidos/segundo, intervalo médio/p95 entre presents (128 amostras), tempo CPU da chamada present, RAM residente do processo (RSS), payload GPU gerido, resolução física, adaptador e contadores de frames submetidos/concluídos. Atualização duas vezes por segundo. CPU submit não mede execução GPU; GPU payload é contabilização de budget, excluindo driver/compositor/buffers adicionais da swapchain, não VRAM medida. Estas métricas não provam scanout nem 4K60.

Clicar abaixo do HUD ativa a câmara: arrastar roda, W/S ou roda do rato aproxima/afasta, A/D ou setas esquerda/direita rodam e setas cima/baixo inclinam. Escape, Tab, clicar nos controlos ou perder foco libertam a navegação e limpam teclas premidas. Atalhos do sistema passam normalmente. É órbita/zoom limitado, não voo livre nem manipulação de objetos. Navegação manual interrompe a órbita automática das malhas; Resume retoma. A simulação Lorenz é independente. As janelas ficam abertas até serem fechadas; E3DN_DEMO_SECONDS=30 (1–600) permite execução temporizada.

Com gpu-macos, enable_scene_input(overlay_height) ativa navegação de estado fixo numa janela gráfica; take_scene_input(elapsed) consome deltas, limita elapsed a 50 ms e informa foco sem expor texto bruto das teclas. native_process_resident_bytes lê RSS atual pela API Mach do sistema; falhas são explícitas. Labels sobre gráficos recebem fundo opaco para leitura.
