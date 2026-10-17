# Conteúdo variável: diagnóstico e plano incremental

> Atualização da fonte: o incremento 1 implementa as correções FMV-01/02/03 e do padding dos blocos, com regressões de região transformada e margem após reflow. Os resultados abaixo preservam o baseline auditado, não a fonte corrigida ainda não publicada. Não houve publicação nem alteração do consumidor. O incremento 2 acrescenta contentores intrínsecos opt-in no corpo; o incremento 3 acrescenta corpos adaptativos de tabela e reserva final do fecho. O incremento 4 acrescenta evidência limitada e opcional de paginação, preserva registos em caso de erro e reutiliza overlays e máscaras SVG.

Data: 2026-10-17. Fonte auditada: `appcore-filemaker 1.0.0-rc.1`.
Estado: diagnóstico e plano; não é uma declaração de correção nem de nova API.
O diagnóstico original abaixo é preservado como evidência do baseline. As correções da fonte ainda não publicada estão descritas acima; a versão publicada e a aplicação consumidora permanecem inalteradas.

## Capacidades existentes que devem ser reutilizadas

| Necessidade | Superfície existente | Limite confirmado |
|---|---|---|
| Medir como o exportador | `TextEngine`, `FontManager`, `measure_content`, `TextLayout` | A medição já inclui shaping, fontes explícitas, largura, padding e entrelinha. Não criar outro medidor. |
| Texto que cresce | `text_options.overflow: expand` | Cresce o elemento de texto; não calcula a altura intrínseca do contentor pai. Com `max_lines`, existe a falha FMV-01 abaixo. |
| Deslocar dependentes | `flow_vertical`, `flow_horizontal`, âncoras para elementos anteriores | O fluxo avança pelas dimensões do pai resolvido; não calcula `max(altura das colunas)` automaticamente. |
| Manter linhas juntas | `keep_together_by`, `keep_with_next` | Chaves iguais devem ser contíguas; grupos grandes dividem por linhas/elementos. Uma linha indivisível maior que a página falha. `group_by` não garante coesão. |
| Corpos diferentes por página | `table.page_bodies.first` / `continuation` | Não existe `last`; campo desconhecido é rejeitado. |
| Cabeçalho/rodapé/fundo por papel | `page.master`, `first`, `continuation`, `last` | Pintados depois da paginação. Não reservam espaço automaticamente. `last` não está ativo num documento de uma página, por contrato atual. |
| Sobreposição local | `collision: { policy: overlay }`, `group`, `collides_with`, `ignore` | Não é preciso desligar colisões globalmente. A auditoria tem a inconsistência FMV-03. |
| Caixas diferentes | `layout`, `intrinsic`, `visual` em `BoundsSet` e `CollisionBounds` | Não confundir área de conteúdo com caixa de layout ou traço visual. |
| Inspeção e segurança | `SceneInspector`, `LayoutTrace`, `audit_layout`, `preflight` | Já há IDs, página, geometria e políticas; faltam razões completas de cada quebra. |
| Visualização | `DebugOverlay`, `CollisionMask`, `MaskView::Combined` | Reutilizar regiões, caixas e máscaras; não ler pixels para decidir layout. |
| Recursos e determinismo | Unidades fixas, limites de páginas/elementos/comparações/reflows, cancelamento | Uma futura convergência global exige orçamento próprio, além dos limites por elemento. |

Implementação: [medição](../src/layout_measure.rs), [texto](../src/text.rs),
[fluxo](../src/layout.rs), [grupos](../src/table_group_pagination.rs),
[regiões de tabela](../src/ir.rs), [camadas](../src/layout_page.rs),
[colisão](../src/collision.rs), [auditoria](../src/validation_layout.rs),
[debug](../src/debug.rs).

## Configuração do consumidor observada

O template indicado foi apenas lido. Os dados reais não foram usados.

- As duas colunas do cabeçalho usam `height: 14mm`, `max_lines: 4`, fonte
  de 9pt e entrelinha 1,25. Quatro avanços de linha somam 45pt, cerca de
  15,875mm, acima da caixa de 14mm. Mesmo quatro linhas podem ultrapassá-la.
- `overflow: wrap` não pede expansão nem redução: o motor pode emitir
  `TextDiagnostic::Clipped`. `min_font_size` não ativa `shrink` por si só.
- Título e corpo usam coordenadas fixas. Aumentar só a caixa de texto não
  desloca os dois; isso não é uma promessa do contrato absoluto existente.
- O adaptador já mede as duas colunas, escolhe a maior altura e substitui
  strings YAML para altura, título e primeiro corpo. É uma compensação
  frágil para uma limitação declarativa real, não uma API que deva ser copiada.
- O corpo declara 267mm, mas os corpos úteis terminam ambos em 260mm da página:
  `16 + 70 + 174` na primeira e `16 + 244` nas seguintes. Os últimos 23mm
  da caixa declarada são excluídos em todas as páginas. Os blocos de fecho são
  elementos posteriores à tabela, não reservas inferidas pelo motor.
- A tabela já declara `keep_together_by: layout_group`. Sem um dataset
  representativo não está provado que todas as descrições/comentários recebam
  a mesma chave contígua; não atribuir esse eventual erro ao agrupador.
- `safe` não equivale a `margin` nem reserva corpo automaticamente.
- `collision: false` global impede o posicionamento/auditoria de verificar
  sobreposições desses elementos. Usar exceções locais no desenho futuro.
- O adaptador chama `export_bytes` e devolve `losses`, mas não chama
  `preflight` ou `audit_layout`. Zero perdas de formato não prova segurança
  de layout. Esta integração pode melhorar usando APIs já existentes.

## Falhas e limitações reproduzidas

As fixtures são genéricas e não contêm dados, nomes ou regras da aplicação.

| ID | Reprodução | Resultado atual | Classificação |
|---|---|---|---|
| FMV-01 | [expand-cap-silent.yml](audit/expand-cap-silent.yml): seis linhas, `expand`, limite quatro | Quatro linhas na cena; nenhum diagnóstico de truncagem; zero perdas PDF e auditoria sem problemas. | Falha do motor: perda sem diagnóstico. Correção deve conservar a intenção existente do limite e reportar a perda, sem mudar implicitamente a semântica de todos os limites. |
| FMV-02 | [next-page-margin.yml](audit/next-page-margin.yml): margem superior 20pt, colisão `next_page` | Elemento vai para a segunda página em `y=0`; a auditoria aceita porque verifica apenas a página física. | Falha de reflow e lacuna de auditoria. Corrigir área de continuação sem impedir fundos/rodapés intencionais. |
| FMV-03 | [allowed-groups.yml](audit/allowed-groups.yml): grupos A/B que não colidem entre si | Posicionamento respeita os grupos; auditoria acusa uma colisão. Não há overflow visual na fixture. | Falso positivo do preflight: reaplicar `CollisionRule::applies_to` com políticas efetivas. |
| FMV-04 | [auto-container.yml](audit/auto-container.yml): pai `height: auto`, duas colunas de alturas diferentes e dependente ancorado | Pai conserva o fallback de uma linha; dependente atravessa texto da coluna maior. Auditoria sem problemas com colisões desativadas. | Limitação semântica: `auto` resolve hoje para tamanho por defeito, não para medição dos filhos. Não mudar silenciosamente V1. |
| FMV-05 | [configured-cap.yml](audit/configured-cap.yml): `wrap` e quatro linhas | Corte com diagnóstico, exportação sem perdas de formato. | Configuração explícita + ausência de integração do preflight; não é um erro de shaping. |

O código de reflow compara o fundo com a altura física e reinicia `y=0` tanto
na passagem de fundo como em `next_page`. A correção deve testar também `push`
contra rodapé/região, não apenas trocar uma constante.

A altura de blocos `keep_with_next` lê `TextLayout.measured.height`, enquanto
a colocação de texto expandido usa `outer_measured()`, que inclui padding.
Existe uma inconsistência a reproduzir antes de corrigir; não foi marcada
como regressão confirmada nesta primeira entrega.

`LayoutTrace` e `SceneInspector::explain_layout` não guardam, por quebra,
capacidade restante, altura do próximo grupo, reservas e regra decisora.
Mensagens novas devem continuar limitadas e não incluir conteúdo dos documentos.

## Evidência e alcance

- Passaram 55 testes existentes em seis suites: `reflow`, `layout`, `table`,
  `pages`, `debug_inspect` e `text`.
- Nove cenários sintéticos executados com a API atual. PDFs híbridos foram
  exportados, rasterizados com Poppler e inspecionados visualmente.
- A matriz [matrix.yml](audit/matrix.yml) com os datasets
  [1 página](audit/matrix-1.data.json), [2 páginas](audit/matrix-2.data.json),
  [5 páginas](audit/matrix-5.data.json) e [20 páginas](audit/matrix-20.data.json)
  gerou exatamente essas contagens, seis linhas de tabela por página.
  Cada linha contém identificação, descrição e comentário. Os índices de origem
  são exatamente `0..N`, sem perda, duplicação ou reordenação.
- Estes controles usam alturas fixas e texto curto; não qualificam o futuro
  crescimento automático, a última página adaptativa, descrições extremas,
  taxas de TVA, remises, preços riscados, totais, códigos e marcas de água juntos.
- PDFs/PNGs, cenas, máscaras, relatórios e logs desta execução estão em
  `target/appcore/filemaker-layout-audit/`. São evidência local, não artefactos
  distribuídos nem testes permanentes que fixam o comportamento defeituoso.
- O índice espacial linear não foi substituído. Não foi medida performance
  release nesta etapa; nenhum ganho de performance é alegado.

## Plano por incrementos e gates

### 1. Regressões e correções compatíveis

Transformar FMV-01/02/03 em testes de comportamento desejado e executar cada um
antes da correção, retendo o resultado vermelho. Reproduzir a inconsistência de
padding em `keep_with_next`. Corrigir sem adicionar campos obrigatórios a
structs públicas ou mudar YAML/valores serializados existentes. Confirmar
`push`, `next_page`, `shrink`, `error`, `overlay`, âncoras e resize com e sem
regiões, margens, transformações e bandas de página. Medir o índice atual em
release para workloads esparsos/densos antes de qualquer proposta de troca.

Gate: regressões verdes, suite completa FileMaker, fmt/check/Clippy, rustdoc,
consumidores e SemVer com baseline apropriado. Corrigir os falsos positivos
antes de recomendar bloqueio estrito para designs com exceções locais.

### 2. Contrato opt-in de medição e crescimento

Reutilizar o mesmo shaping e medição. Texto automático mantém largura definida;
uma coluna vertical soma alturas e gaps; duas colunas horizontais usam a maior
altura. Medir antes de publicar âncoras e avançar o fluxo. Proibir dependências
circulares como percentagens de altura sem base definida num pai intrínseco.
Definir largura, padding, entrelinha, min/max, texto vazio e limite indivisível.
Preservar o mínimo de 14mm do design curto por intenção de mínimo, não por
coordenadas substituídas em Rust. Corte, expansão, redução e erro ficam
explícitos; limites nunca apagam texto sem evidência.

Não atribuir nova semântica silenciosamente a `height: auto` no V1 publicado.
Adicionar campos a `ElementSource`/`TableSource` quebra literais Rust mesmo com
`serde(default)`. Propor tipos/entrada opt-in separados e plano de migração e
rollback; uma eventual nova versão de documento não deve alterar o parser V1
nem recriar o medidor. A numeração das crates continua independente e em RC.

Gate: cabeçalho curto igual ao anterior; textos longos completos nas duas
colunas; título/corpo deslocados em YAML; zero remendos de YAML no consumidor.

### 3. Regiões primeira/intermédia/última e convergência

Reutilizar o paginador e grupos existentes. Distinguir região física, conteúdo,
reservas e geometria pintada. Declarar a reserva uma única vez, só quando a
banda está ativa. Definir explicitamente o caso de página única; não ativar
`PageRole::Last` em V1 silenciosamente. Grupos que cabem são indivisíveis;
grupo maior que uma página usa política declarada de divisão entre linhas ou
erro, sem uma página vazia de tentativas.

Recalcular se a distribuição mudar o papel da última página. Usar unidades
fixas, ordem estável, orçamento global de passes/medições e cancelamento.
Detetar estados repetidos da distribuição/reservas e falhar com causa; não
forçar convergência criando páginas vazias ou reservando o fecho duas vezes.

Gate: 1/2/5/20 páginas, casos limite da última página, sem perdas/reordenação,
sem títulos órfãos, sem espaço reservado em páginas sem o bloco correspondente.

### 4. Explicações e segurança do layout

Reutilizar `audit_layout`, preflight, inspeção e overlays. Acrescentar um
relatório opt-in de decisões por quebra com página, IDs/índices, região,
altura disponível, altura do grupo, reservas e política. Identificar geometria
anterior/final e parceiros de colisão; manter budgets e omitir conteúdo pessoal.
Preservar o formato dos relatórios publicados; novos detalhes precisam de
superfície versionada separada se alterarem o contrato.

Gate: relatórios JSON determinísticos, truncagem fail-closed, ciclos e limites
compreensíveis; exportar com sucesso não substitui a auditoria de layout.

### 5. Corpus comercial e adoção do consumidor

Fixtures sintéticas com textos extremos, artigos/descrições/comentários,
várias taxas, remises detalhadas, preços riscados, totais, EAN/QR, carimbos e
marcas de água. As regras fiscais e o cálculo dos valores ficam nos dados da
aplicação; o motor recebe apenas conteúdo, estilo, grupos e geometria genéricos.
Regenerar PDFs, renderizar todas as páginas e comparar cenas/PNGs e ordem do
conteúdo. Verificar também HTML/SVG/raster contra a mesma cena.

Só depois substituir a compensação de strings do consumidor por YAML opt-in,
com teste do cabeçalho curto, do longo e rollback para a dependência anterior.
Não eliminar os patches externos antes de provar equivalência do pacote.

Gate final: critérios de aceitação completos, documentação EN/PT/FR alinhada,
novo RC independente qualificado. Esta auditoria não publica ou instala pacotes.

## Reproduzir com a CLI existente

Executar na raiz da workspace. Este comando passou sem issues no controle de 20 páginas. Passar numa fixture defeituosa não prova segurança; comparar os resultados acima.

```bash
cargo run -p appcore-filemaker-cli --locked -- \
  --json --strict \
  --font Body=crates/appcore-filemaker/examples/assets/NotoSans-Regular.ttf \
  --data crates/appcore-filemaker/wiki/audit/matrix-20.data.json \
  preflight crates/appcore-filemaker/wiki/audit/matrix.yml \
  --format pdf --pdf-mode hybrid
```

## Validação do incremento 1 — 2026-10-17

As correções compatíveis estão implementadas na 1.0.0-rc.2. Onze testes
novos cobrem truncagem, avisos nos fragmentos, padding dos blocos, continuação
em margens/regiões, transformações do pai, validação de corpo importado após
reflow, ordem de overlay, rodapé e âncoras após shrink. As falhas foram retidas
antes de cada correção; duas fixtures antigas de fiabilidade passam a representar
sobreposição importada não autorizada em vez de overlay permitido. A suite
completa FileMaker passa 236 testes; consumidores AI/CLI passam 47. Check/Clippy
da workspace, rustdoc, norm e formatação passam. Comparação patch contra
`1.0.0-rc.1` publicado: 223 checks SemVer aplicáveis passam (30 ignorados).
Não foram acrescentados campos públicos ou valores de protocolo.

Os controles PDF de 1/2/5/20 páginas regenerados são idênticos ao baseline byte
por byte; índices e identificadores extraídos do PDF continuam completos e
ordenados. Baseline local em release: `reflow_dense_64` tem média de 1,412ms
por iteração e `diagnostic_geometry_256` de 1,852ms, dez iterações cada.
São workloads distintos, não comparação de ganho, estudo de percentis ou budget
de produção. O índice linear permanece inalterado. Evidências em
`target/appcore/filemaker-compatible-fixes/`. Continuam abertos os gates de
altura intrínseca, última página convergente e corpus comercial completo.

## Altura intrínseca (opt-in, 1.0.0-rc.2)

[Altura intrínseca (opt-in, 1.0.0-rc.2)](intrinsic-height.pt.md). `LayoutEngine::resolve_intrinsic` — `examples/intrinsic-height.yml`.

## Paginação adaptativa de tabela (opt-in, 1.0.0-rc.2)

[Paginação adaptativa de tabela (opt-in, 1.0.0-rc.2)](adaptive-pagination.pt.md). `AdaptiveLayoutPlan` / `LayoutEngine::resolve_adaptive`.

Diagnóstico opcional e limitado: `diagnose_adaptive` devolve decisões numéricas e conserva os registos anteriores em caso de erro. Ver o guia de paginação adaptativa.

[Corpus sintético misto e gate de adoção pelo consumidor](variable-content-corpus.pt.md).
