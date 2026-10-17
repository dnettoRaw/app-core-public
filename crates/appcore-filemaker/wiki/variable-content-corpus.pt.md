# Corpus misto de conteúdo variável (1.0.0-rc.2)

A release candidate `1.0.0-rc.2` inclui as alterações de conteúdo variável, altura intrínseca, paginação adaptativa, diagnóstico, ordem de pintura e Unicode PDF abaixo. Notas anteriores de código não publicado descrevem etapas de desenvolvimento. Selecionar o pacote exato após verificação; a migração do consumidor continua separada.

Executa `cargo run -p appcore-filemaker --example variable_corpus --locked`.
Um desenho YAML e um plano adaptativo separado recebem quatro datasets JSON
tipados e sintéticos. Os outputs `target/filemaker-examples/variable-corpus-*`
incluem PDF, cena, preflight estrito, decisões, overlay e máscaras SVG.
Este incremento também corrige a precedência de pintura das bandas no motor existente. Sem novas dependências, API pública, campos serializados ou versão publicada.

| Páginas | Grupos | Linhas de origem |
|---|---|---|
| 1 | 3 | 9 |
| 2 | 12 | 36 |
| 5 | 40 | 120 |
| 20 | 190 | 570 |

Cada grupo contém artigo, descrição variável e comentário com remise detalhada.
As três linhas partilham `keep_together_by`; `oversized_group: error` rejeita
explicitamente uma descrição que exceda uma página. O teste verifica texto lógico
no shaping e altura medida das células, além das strings de origem.
O cabeçalho automático em duas colunas, o título dependente e o primeiro corpo
partilham a medição nativa. Três taxas de TVA, valores anteriores, remises,
totais líquidos/fiscais/finais, condições, assinatura e EAN-13 são dados fornecidos
pelo consumidor sintético. O FileMaker não calcula impostos nem valida regras
comerciais. Os totais das fixtures são coerentes, mas o corpus não é certificação fiscal.

O primeiro valor anterior tem uma linha sobre a célula, com overlay local explícito.
A marca de água em todas as páginas e o carimbo da primeira usam bandas existentes:
essas bandas já têm contrato de pintura sem colisão e rejeitam um campo `collision`
explícito. A deteção de colisão do corpo permanece ativa. Âncoras esparsas devem
ser omitidas ou null quando ausentes, não strings vazias repetidas.
As coordenadas X do corpo são relativas à origem da região útil; uma regressão
confirma que o risco fica dentro da célula pretendida depois desse deslocamento.
Foi uma correção de configuração da fixture, não uma falha do motor ou do shaping.

Os testes verificam ordem sem perda/duplicação, texto integral nas células, grupos
intactos, número de páginas, geometria do risco e descendentes do fecho apenas na
última página, incluindo código e assinatura. Executam `audit_layout` e preflight
estritos. As páginas também devem ser renderizadas e revistas: zero perdas de
exportação não aprova o layout. A falha anterior de extração de ligaturas está corrigida na exportador 1.0.0-rc.2, com clusters completos e `/ActualText` contextual. Continua a conferir extração e renderização separadamente. Máscaras SVG mostram ocupação geométrica;
`audit_layout` aplica as políticas de sobreposição intencional.

## Gate de adoção pelo consumidor

Um consumidor fixado no pacote do registry não executa métodos só presentes na
fonte. Não aplicar patches locais nem alterar a dependência silenciosamente antes
de publicar e qualificar uma nova RC independente. Preparar a adoção assim:

1. Selecionar explicitamente a nova RC exata do registry quando estiver disponível.
2. Mover identificação variável e título para raízes do corpo; conservar bandas
   decorativas fixas. Definir `height: auto`, duas colunas e âncoras no YAML.
3. Substituir reservas fixas por plano adaptativo, `first_after` e raiz final de
   fecho. Remover `reserve_after` concorrente e totais internos da tabela nesse
   desenho selecionado; os totais continuam a ser dados vinculados.
4. Chamar `resolve_adaptive` ou `diagnose_adaptive`, seguido de auditoria e preflight
   estritos. Remover medição manual do cabeçalho e substituição de coordenadas YAML
   apenas depois de conferir as regressões sintéticas do consumidor real.
5. Conferir fontes nativas, cabeçalhos curtos/longos, fechos condicionais, remises,
   códigos e outputs renderizados. Conservar release anterior, template e integração
   da aplicação como artefacto explícito de rollback.

O consumidor externo não foi alterado. Sem publicação de pacotes ou deploy da wiki.

Reproduz a verificação após executar o exemplo: `python3 crates/appcore-filemaker/wiki/audit/verify-variable-corpus.py`. Exige Poppler, confere IDs ordenados de artigos/descrições/comentários, linhas de totais fornecidos, os 95 módulos EAN-13 renderizados e zonas de silêncio, e renderiza as 28 páginas. Não substitui revisão visual nem certificação de scanner.

A marca de água pertence a `page.master.background`. `z_index` ordena a pintura dentro de uma camada; um valor negativo em `header` não coloca a marca atrás dos glifos do corpo. A regressão falhou primeiro com a banda de cabeçalho e verifica agora a camada de fundo e a ordem de pintura. O preflight estrito aprovava a configuração que tapava o texto.

O corpus também revelou uma falha do motor: a ordenação textual colocava a camada vazia do corpo antes de `!page-background`, e nomes Unicode podiam ficar depois da banda de primeiro plano. A correção compatível dá precedência explícita às bandas: fundo, corpo, cabeçalhos/rodapés. Os nomes das camadas do corpo conservam a ordem textual, seguida de z-index e sequência. Os testes cobrem camadas vazias/personalizadas/Unicode, valores extremos de z-index e pixels raster reais. Sem substituir o índice espacial ou criar API.

Ao adotar a correção de pintura, recalcula as cenas em cache; não reutilizes o cache de um binário anterior da fonte. O rollback seleciona explicitamente a release qualificada anterior da aplicação/crate.
