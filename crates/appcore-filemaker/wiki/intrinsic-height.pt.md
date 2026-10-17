# Altura intrínseca (opt-in, 1.0.0-rc.2)

A release candidate `1.0.0-rc.2` inclui as alterações de conteúdo variável, altura intrínseca, paginação adaptativa, diagnóstico, ordem de pintura e Unicode PDF abaixo. Notas anteriores de código não publicado descrevem etapas de desenvolvimento. Selecionar o pacote exato após verificação; a migração do consumidor continua separada.

Usa `LayoutEngine::resolve_intrinsic(&document)` em vez de `resolve` para
medir os elementos com `height: auto` explícito no YAML. A chamada `resolve`,
o parser V1, os literais das estruturas públicas e as políticas de overflow
com altura fixa conservam o contrato. Não há dependências novas nem medição
manual no consumidor.

O texto usa o shaping e a medição existentes, com a fonte registada, estilo,
largura, padding e entrelinha efetivos. Contentores horizontais usam a maior
altura; os verticais somam alturas e espaçamentos. Define
`constraints: {min_height: 14mm}` para preservar o desenho curto. Fluxos e
âncoras recebem a caixa medida antes da resolução de colisões. O IR de entrada
não é alterado. Conteúdo acima de `max_height` produz erro com o ID.

Esta primeira opção suporta larguras definidas e fluxos do corpo distribuídos
no início. Texto automático aceita `wrap`, `expand` ou `error`, horizontal e
sem `max_lines`. Para `clip`, `ellipsis` ou `shrink`, usa altura fixa explícita.
Contentores automáticos são indivisíveis e devem caber numa região útil de
uma página. Os filhos não podem ter x/y, âncoras, região, transformações,
alinhamento geométrico diferente de início, keep-with-next nem alturas ou
restrições percentuais de altura. Texto filho com expansão deve usar altura
automática. Contentores absolutos/distribuídos, escrita vertical, bandas de
página automáticas, aspect ratio/preferred height no elemento automático e
padding vertical percentual produzem erro explícito. Assim não se estimam
silenciosamente dependências circulares.

A passagem verifica cancelamento, limites de elementos/texto e profundidade
máxima de 64 antes de clonar o documento ligado aos dados. A medição verifica
cancelamento por elemento; shaping de um elemento não é preemptivo. Há custo
adicional de cópia e medição, sem alegação de melhoria de desempenho ou RAM.
Executa preflight de layout na cena final, incluindo elementos fixos e overlays.
Não reutilizes chaves de cache do layout convencional para esta opção.

Executa `cargo run -p appcore-filemaker --example intrinsic_height --locked`.
`examples/intrinsic-height.yml` usa os JSON curto/longo separados, duas colunas,
texto alinhado à direita e âncoras para deslocar título e corpo. Os resultados
ficam em `target/filemaker-examples/`. O Rust não substitui coordenadas YAML.
O exemplo exige diagnósticos de layout válidos antes de exportar PDF.

Migração: opta pela nova chamada e declara alturas automáticas/mínimas no YAML.
Rollback: conserva o desenho original e usa `resolve`. A API Rust é aditiva,
sem outro parser ou versão YAML. Reservas adaptativas de primeira/páginas
intermédias/última de tabela usam agora o plano adaptativo separado; fluxos globais arbitrários ficam fora desse contrato.

Nas raízes que participam na medição intrínseca, os contentores são estruturais:
as políticas de colisão dos descendentes ignoram explicitamente os IDs dos
ancestrais. Colisões entre irmãos e com elementos externos continuam ativas.
A resolução convencional e as raízes sem altura automática mantêm as políticas.

A falha anterior de extração de ligaturas está corrigida na exportador 1.0.0-rc.2, com clusters completos e `/ActualText` contextual. Continua a conferir extração e renderização separadamente.
