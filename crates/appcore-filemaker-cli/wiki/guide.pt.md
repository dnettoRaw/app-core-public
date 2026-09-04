# Guia do appcore-filemaker-cli

Este adapter de processo limitado compila o mesmo YAML estrito e usa a mesma
cena resolvida da API Rust. O formato de export é escolhido somente pelo
comando, nunca pelo template.

Use `check` para validar schema, `validate` para layout vinculado, `preflight`
para um request de exporter e `render` para output atômico PDF, SVG, PNG, JPEG,
HTML ou CSV tabular. `inspect`, `explain`, `free-regions`, `debug` e `mask` são
fronteiras de diagnóstico.
`schema` e `capabilities` são
somente leitura. `migrate` é reservado e retorna unavailable sem mudar input.

`schema --json` expõe cores tipadas, a cascata completa de style, overrides de
export somente de pintura e o fato de layer/z-index nunca controlar colisão.
Ele também lista unidades de coordenada, primitivas, comandos de path e
gráficos avançados preparados do Canvas; templates nunca codificam uma
superfície pintada em pixels.

Use `debug TEMPLATE --grid 1|5|10|20 --view combined` para o overlay completo e
não mutante. `mask` exporta geometria collision/layout/visual/combined como
PNG, PDF, SVG ou JSON. O JSON separa occupied, free, collisions e overflow.
`inspect` e `explain` retornam geometria de origem, anchors, region, medição,
colisão, página/reflow e provenance preservadas.
`free-regions TEMPLATE --minimum-width 20pt --minimum-height 10pt` retorna os
retângulos resolvidos e limitados que comportam esse tamanho mínimo.

`capabilities --json` expõe PDF editável, flattened e híbrido. Hybrid desenha
outlines vetoriais determinísticos e adiciona texto Unicode invisível e subsetado
para busca, seleção e extração. WebP, XLSX, ZPL, ESC/POS, PDF/A, links,
bookmarks e acessibilidade tagged continuam preparados.
A autodescrição cobre writer/bytes limitados, loss reports strict/best-effort,
DPI somente raster, metadados PDF determinísticos e subsets de fonts.

Passe dados com `--data`, fonts com `--font NAME=FILE` repetível, a ordem exata
de fallback com `--font-fallback NAME` repetível e um sandbox explícito com
`--assets-root`. Aplique patch files ordenados com `--patch FILE` repetível.
Para CSV use `render TEMPLATE --format csv --table ELEMENT --output FILE`; se
houver uma só tabela, `--table` pode ser omitido. Use `--json` para automação
estável e preserve exit codes diferentes de zero.

Todo comando emite texto humano conciso por default e JSON estável com
`--json`. A descoberta de capabilities publica exit codes 0 (sucesso), 2
(validação), 64 (uso), 65 (dados), 66 (input ausente), 69 (indisponível), 70
(software), 73 (não pode criar), 74 (I/O), 75 (falha temporária de recurso) e
130 (cancelado).
Os dois modos terminam com uma newline e compartilham teto de stdout de
512 MiB. Pretty JSON é dimensionado primeiro e serializado direto por um buffer
fixo de 16 KiB, sem exigir uma segunda string completa para automação.

Escritas de artifact usam arquivo temporário exclusivo, sync dos dados e rename
atômico. `render` e `mask` rejeitam output que resolve para o template de input.
`migrate` está indisponível e não muta; uma migração futura não poderá escrever
sem um novo flag e contrato explícitos.

`check`, `validate` e `preflight` mantêm separados diagnósticos de schema,
layout resolvido e exporter. JSON inclui issues limitadas e `truncated`
explícito; strict rejeita warnings e truncamento sempre falha fechado.
`schema --json` também lista validação de dados tipados, inputs completos do
fingerprint e cache imutável limitado resolve-on-miss.

Leituras de template, dados e fonts permanecem em um único handle aberto e
param após `limit + 1` bytes. Overlays de debug e masks reutilizam os limites
do core do comando, inclusive o budget de comparações e geometria retida.
