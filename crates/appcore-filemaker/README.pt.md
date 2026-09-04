# appcore-filemaker

[English](README.en.md) | [Français](README.fr.md)

Compilador determinístico do AppCore para documentos declarativos, canvases
vetoriais semânticos e datasets limitados. O YAML versionado
`filemaker: "1.0"` é apenas um frontend: compilação, binding de dados, layout,
colisão, inspeção, preflight e export continuam fases explícitas.

O crate usa geometria fixed-point, resolvers explícitos de fontes e assets,
recursos limitados, cenas resolvidas imutáveis e falhas tipadas. O formato é
escolhido na chamada de export, nunca no YAML. O crate não depende de
`appcore-ai`; o bridge opcional e a CLI ficam em crates separados.

O shaping de texto usa somente bytes de fonts registradas. A lista ordenada de
fallback faz parte do fingerprint, e o embedding SVG/HTML segue as fonts dos
glyph runs resolvidos. Patches de runtime são aplicados antes da medição e do
layout, portanto a geometria é sempre recalculada a partir do IR alterado.
O JSON canônico do fingerprint é dimensionado e hasheado em duas passagens por
writer sob o budget agregado `max_output_bytes`; os bytes V1 permanecem
idênticos sem reter um segundo buffer JSON completo.
`text_options.writing_mode: vertical` molda colunas de cima para baixo que
avançam da direita para a esquerda. Medição e quebra acontecem uma vez no
layout; PDF, SVG, PNG/JPEG e HTML consomem as mesmas colunas e runs moldados.

Em processos long-lived, use os construtores de `OperationLog` e `SceneCache`
limitados por bytes, `BorrowedDataset` para linhas que já estão em memória e a
API writer. PNG e JPEG renderizam faixas verticais limitadas e as codificam
diretamente nesse writer; o PNG da máscara de colisão usa o mesmo caminho, e o
raster completo e o output codificado nunca coexistem na memória. CSV, SVG e
HTML também transmitem incrementalmente. PDF faz uma passagem limitada de
dimensionamento e então emite objetos independentes e sua tabela de referências
cruzadas rastreada sem reter um buffer final do documento.
JSON, SVG e PDF da máscara de colisão seguem a mesma regra de dimensionamento
antes da escrita e serializam direto no writer do chamador. PDF emite objetos
independentes, um content stream de tamanho exato e seu xref clássico sem reter
o stream da página nem o arquivo completo; o helper JSON que retorna bytes
dimensiona primeiro e aloca somente o resultado exato aceito.

PDF suporta texto editável, flattened e híbrido. O modo híbrido desenha outlines
determinísticos das fonts para a aparência e adiciona uma camada Unicode
invisível e subsetada para busca, seleção e extração, sem reflow no exporter.
O planejamento de fluxo distribuído conta os filhos visíveis sem alocar uma
lista temporária de referências, preservando os mesmos cálculos de tamanho e
espaçamento.
A coleta de nomes de assets no fingerprint ordena referências emprestadas,
evitando clonar strings durante a resolução determinística.

O benchmark runtime do crate expõe workloads separados `compile_canvas_yaml`,
`fingerprint_json_4m`, `collision_mask_json_4m`, `a4_report_end_to_end` e
`a4_report_pdf_hybrid`. `a4_report_export_matrix` executa o mesmo pipeline de
duas páginas com YAML/dados/patch/medição/layout/colisão e então faz preflight e
stream dos três modos PDF, SVG, HTML semântico e fixo, PNG, JPEG e CSV do dataset
para sinks sem retenção. Ele mediu 70,56 ms p50, 71,34 ms p95, MAD de 0,22 ms e
10,64 MiB de RSS pico no Apple M1. `collision_mask_pdf_100k` também grava um PDF
de 1.800.626 bytes a partir de 100.000 retângulos resolvidos; o caso JSON da
máscara grava 4.188.826 bytes em um sink sem retenção.
A resolução de camadas de página agora percorre os elementos ativos de forma
lazy em cada página física, sem lista temporária de referências e preservando a
ordem das roles.

```bash
cargo run -p appcore-filemaker --example basic
cargo run -p appcore-filemaker --example intermediate
```

Cada runner Rust carrega um documento `.yml` separado em `examples/`; o YAML do
template não fica embutido no código Rust. O runner básico grava um SVG completo
de uma página; o intermediário grava PDF de duas páginas, HTML fixo, previews
SVG por página e um relatório de preflight estrito em
`target/filemaker-examples/`. Os dados tipados também ficam em arquivos JSON
separados, e a fonte Noto Sans exata, sob OFL, acompanha o exemplo para um
resultado portátil e determinístico. Veja a
[arquitetura](wiki/architecture.pt.md), o [exemplo básico](wiki/examples/basic.pt.md)
e o [exemplo intermediário](wiki/examples/intermediate.pt.md).

Licença: MIT.
