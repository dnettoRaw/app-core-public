# Paginação adaptativa de tabela (opt-in, 1.0.0-rc.2)

A release candidate `1.0.0-rc.2` inclui as alterações de conteúdo variável, altura intrínseca, paginação adaptativa, diagnóstico, ordem de pintura e Unicode PDF abaixo. Notas anteriores de código não publicado descrevem etapas de desenvolvimento. Selecionar o pacote exato após verificação; a migração do consumidor continua separada.

`AdaptiveLayoutPlan::parse_yaml` lê um desenho YAML separado e estrito com
`filemaker_layout: '1.0'`. Depois do compile/bind V1 existente, chama
`LayoutEngine::resolve_adaptive(&document, &plan)`. A API é aditiva: preserva
literais das estruturas públicas, parser V1, `resolve` e `resolve_intrinsic`.
Não acrescenta dependências, formatos persistentes nem regras comerciais.

O plano seleciona uma tabela raiz, corpos verticais `first`, `intermediate`,
`last` e `single`, um bloco raiz final `closing`, o espaçamento e a política
obrigatória `oversized_group: error` ou `split_rows`. Offsets e alturas são
relativos à caixa completa da tabela, com largura constante. Os quatro corpos
devem caber nessa caixa; a caixa deve caber na região útil da página.

Os corpos declarados são ANTES da reserva do fecho. A altura visível do fecho
usa a medição intrínseca e as restrições existentes. Altura mais espaçamento
são subtraídos uma única vez de `single` ou `last`. O paginador existente
subtrai depois o cabeçalho da tabela uma única vez. Fecho oculto não reserva
altura nem espaçamento. As bandas decorativas mantêm a semântica V1 e não
criam reservas implícitas.

O grupo pendente existente permite reconhecer o grupo final. Só a sua última
linha transporta a diferença entre capacidade normal e final. Quando a linha
ou o grupo mudam de página, a reserva e a medição sensível à página são
recalculadas no destino. O fragmento final é deslocado para o corpo last/single;
as âncoras existentes posicionam o fecho depois do seu fundo. Para este problema
restrito de tabela vertical não é preciso estimar o número de páginas nem
iterar um ponto fixo global. Amostragem de colunas e medição de grupos ainda
podem visitar linhas várias vezes; não há promessa de uma única leitura.

Chaves contíguas iguais em `keep_together_by` permanecem juntas quando grupo e
reserva final cabem numa página de destino. `split_rows` divide entre linhas
quando esse requisito conjunto não cabe; `error` rejeita-o. Uma linha
indivisível que não caiba com a reserva falha nas duas políticas. Se um grupo
deixar a primeira página completamente vazia, há erro explícito, sem devolver
página vazia nem reclassificar silenciosamente uma continuação. Um dataset
vazio pode gerar cabeçalho e fecho, mas o cabeçalho deve caber.

Limites desta opção: uma tabela visível raiz que começa na página física zero;
um único bloco final do corpo ancorado por `top: table-id.bottom+gap`; essas
raízes não podem usar transformações, regiões ou keep-with-next. O fecho pode
ser texto ou contentor de fluxo automático. Os descendentes devem caber na
caixa medida e na página final da tabela. Colisão incompatível ou crescimento
de texto fixo produz erro sem cena parcial. Não combinar `table.page_bodies`,
`reserve_after` condicional ou `total_fields` interno com este dono da reserva.
Totais podem ser dados ligados no bloco de fecho. Várias tabelas, larguras
finais diferentes e fluxos globais dependentes da página exigem outro contrato
explícito, sem inferência.

Continuam ativos os limites de bytes, profundidade, elementos, linhas, páginas
e trabalho de colisão. O cancelamento é verificado antes do planeamento, na
visita das linhas, medição e avaliação de reservas; um shaping individual não
é preemptivo. O índice espacial é o mesmo, sem alegação de ganho de tempo ou
RAM. Valida layout e exportação separadamente: sucesso ou zero perdas declaradas
não provam geometria nem fidelidade da camada textual. A falha anterior de extração de ligaturas está corrigida na exportador 1.0.0-rc.2, com clusters completos e `/ActualText` contextual. Continua a conferir extração e renderização separadamente.

Executa `cargo run -p appcore-filemaker --example adaptive_pagination --locked`.
`examples/adaptive-pagination.yml`, `adaptive-pagination-plan.yml` e JSON
tipados separados geram exemplos de 1/2/5/20 páginas em `target/filemaker-examples/`.
O exemplo exige auditoria de layout e preflight antes de escrever cada PDF.
A regressão de oito linhas passa de três páginas para duas, com uma única
reserva de fecho e índices de origem ordenados e únicos. O consumidor não mede
texto nem substitui coordenadas em strings YAML no Rust.

Migração: guarda o desenho original, move o desenho do corpo para o plano,
remove reservas V1/por linha concorrentes, declara a âncora do fecho e seleciona
a nova chamada. Rollback restaura o desenho e a chamada anteriores; dados e
formatos persistidos mantêm-se. Incluído em `1.0.0-rc.2`; 
a aplicação externa não foi migrada.

`first_after: {element: title, gap: 8pt}` permite iniciar `first` e `single`
abaixo de uma caixa de layout já medida na página física zero. Conserva os
fundos declarados e nunca sobe o início acima do offset original. Liga assim
o cabeçalho automático de duas colunas e o título à região da tabela, sem
aritmética de coordenadas no consumidor. A caixa completa da tabela pode
sobrepor essas caixas anteriores, embora o corpo útil selecionado não o faça:
declara os IDs em `collision.ignore` local, como em `examples/adaptive-header.yml`.
Não é preciso desativar colisões globalmente. Alvo ainda não resolvido, posterior,
noutra página ou corpo inicial esgotado produz erro explícito. Um dataset não
vazio deve colocar linhas na primeira página; um adiamento inicial que deixe
apenas títulos é rejeitado para não devolver um título isolado.

Executa `cargo run -p appcore-filemaker --example adaptive_header --locked`
para os cabeçalhos curto/longo. O mesmo YAML e plano usam JSON sintéticos
separados, com auditoria de layout e preflight obrigatórios. Os exemplos de
paginação e cabeçalho geram juntos seis documentos PDF para inspeção visual.

## Evidência limitada de paginação (adição 1.0.0-rc.2)

Usa `LayoutEngine::diagnose_adaptive(&document, &plan, max_decisions)` no lugar
de `resolve_adaptive` quando precisas de uma explicação. `AdaptiveLayoutReport`
expõe `outcome()`, `decisions()` e `into_outcome()`. Consulta ou guarda os registos
antes de propagar o resultado: a evidência anterior permanece disponível em caso
de erro. Cada chamada tem registos independentes. As APIs e os literais públicos
existentes mantêm-se; a resolução normal não cria um coletor.

Cada `PaginationDecision` contém o ID da tabela, página física candidata e
ordinais das linhas de origem (base zero, intervalo inclusivo), regra real e
alturas numéricas. `capacity` desconta o cabeçalho da tabela uma vez;
`used_height` inclui linhas e reservas anteriores;
`available_height = capacity - used_height`;
`required_height = group_height + reserved_height`. `final_reservation` já está
incluída em `reserved_height`: não a somar outra vez. Combina fecho e espaçamento
com a diferença entre alturas do corpo normal e final, podendo exceder a altura
do fecho isoladamente.

As regras são `fit`, `move_group`, `split_rows`, `move_row`, `skip_first_page`
e `reject`. Um avanço descreve o candidato na origem; o `fit` posterior usa
medição e reserva do destino. A decisão sobre grupo excessivo avalia uma página
de continuação vazia, mesmo que essa página nunca seja emitida. Os registos
mostram execução, não são uma lista de páginas emitidas nem garantia de sucesso.
Validação anterior à paginação pode falhar sem registos; uma validação posterior
pode conservar decisões `fit` sem devolver cena. Dataset vazio não produz
decisões de linhas. O enum de regras é não exaustivo.

O limite explícito tem de estar entre um e `ResourceLimits.max_elements`.
Esgotá-lo devolve `LimitExceeded` e conserva o prefixo, sem truncar silenciosamente
nem devolver cena parcial. Não guarda texto, payloads nem valores de agrupamento,
apenas IDs, ordinais e geometria. O coletor opcional usa secções curtas de mutex,
preserva as características Send/Sync do motor e acrescenta custo de alocação e
serialização. Cancelamento e limites existentes de páginas, linhas e medição
continuam ativos.

`cargo run -p appcore-filemaker --example adaptive_pagination --locked` grava
`*-pagination.json`, `*-debug.json` e `*-mask-*.svg` por página, junto dos exemplos
de 1/2/5/20 páginas. Reutiliza `DebugOverlay::build_bounded` (área útil, regiões,
caixas de layout/intrínsecas/visuais e âncoras) e o exportador SVG de
`CollisionMask`. A máscara descreve ocupação geométrica; `audit_layout` aplica
as políticas efetivas de sobreposição. `SceneInspector::explain_layout` explica
a geometria final; `audit_layout` identifica colisões e cortes com geometria.
Continua a usar preflight estrito e revisão do PDF renderizado: zero perdas do
exportador não prova um layout seguro. Rótulos do overlay podem conter expressões
da fonte; revê-os antes de partilhar externamente.

Rollback: volta a `resolve_adaptive` e deixa de guardar estes outputs.
Sem alterações ao YAML, persistência, dependências, índice espacial ou consumidor.

`header_height` mostra a reserva de cabeçalho aplicada; `body_height = capacity + header_height` mostra o corpo normal antes desse desconto.
