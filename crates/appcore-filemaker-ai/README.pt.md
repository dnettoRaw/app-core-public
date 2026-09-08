# appcore-filemaker-ai

Erros de conversão JSON retêm até 512 bytes sem cortar caracteres UTF-8 nem
manter um buffer de mensagem grande demais. Argumentos Unicode inválidos
retornam erro controlado e preservam documento e revisão; a call rejeitada
consome seu budget. Serde pode ter alocado a mensagem completa antes; isso não
é garantia de pico de alocação nem de redaction de segredos.

`filemaker_validate` também conta o envelope completo emprestado antes de montar
JSON, incluindo mensagens, escapes do template e truncamento. Warnings continuam
válidos se não houver erros ou truncamento; relatório truncado nunca vira
`valid: true`. O relatório do core ainda é construído antes, dentro do limite de
issues: isso evita a árvore JSON rejeitada, não o próprio relatório diagnóstico.

A admissão de nomes de tools e a validação da lista permitida usam contratos
estáticos de argumentos, sem reconstruir todos os schemas JSON públicos nem
reter um cache global. Quando solicitado para descoberta pelo modelo,
`tool_definitions()` copia nomes, descrições e schemas serializados exatos dos
contratos estáticos; não monta mais árvores JSON nem vetores intermediários por
categoria. Nomes desconhecidos falham antes
da contagem de calls; tools conhecidas mantêm os checks de policy, argumentos e budget.

`filemaker_capabilities` conta uma visão emprestada do contexto do documento,
limites e policy antes de montar a árvore JSON. Contexto purpose/rules/editable/
locked grande demais é rejeitado sem clonar essas coleções para JSON. Respostas
aceitas preservam campos, ordem das listas e contagem exata dos bytes escapados;
uma sessão vazia continua retornando `document_context: null`. A árvore de saída
continua owned, e chamadas rejeitadas consomem o budget de calls. Isso não limita
memória residente da sessão, construção de diagnósticos nem scratch de exporters.

O benchmark `runtime` inclui `create_patch_256_elements`: uma sessão nova cria
um Canvas de 256 retângulos, oculta um elemento por tool e inspeciona outro,
verificando revisões e resultados. Compile/bind e criação do JSON fixture ficam
fora do tempo; parsing JSON, conversão tipada, dois layouts, checks de policy/
resultado e destruição da sessão ficam dentro. Mede edição, não shaping de
texto, exporters, latência de cancelamento ou todas as tools.

Argumentos tipados de mutação, elementos source, comprimentos e overrides de
estilo são desserializados da árvore JSON existente, sem cloná-la primeiro.
O IR/patch resultante ainda possui suas strings e coleções; isso não significa
parsing sem cópias do texto JSON original.

Respostas de mutação são dimensionadas antes do commit de documento/cena.
Se `max_result_bytes` não comportar a resposta, create/load/patch e ferramentas
de edição derivadas retornam erro de policy sem mudar documento ou revisão.
A tentativa consome o budget de calls; a validação do candidato pode já ter
ocorrido. Esse budget de resposta não reserva memória temporária do layout.
O teto cobre o `ToolExecution` serializado inteiro, incluindo `tool`, `revision`
e `value`. Builders recebem somente o budget exato restante do value, então uma
rejeição do envelope externo não ocorre depois do commit de uma mutação.

`export_dataset_csv_controlled` aceita `OperationControl`, verifica cancelamento
antes da saída e nas fronteiras de linhas, e reporta linhas concluídas na fase
Export. Retorna `Cancelled` ao cancelar; o writer pode conter um prefixo CSV
parcial que o chamador deve descartar ou reverter. Callbacks de dataset/writer
são cooperativos, não preemptíveis. A bridge AI usa o mesmo controle para CSV.

Use `FileMakerAiSession::with_control(OperationControl)` para compartilhar
cancelamento e progresso com layout/reflow, validação/preflight e export gráfico.
Instale em `empty(...)` antes de create/load para controlar o layout inicial;
`new(...)` valida antes de uma chamada posterior ao builder. Chamadas canceladas
consomem o budget de calls. Um candidato cancelado no layout restaura cena e
revisão anteriores. Trocar controles não reinicia policies nem budgets.
Consultas de regiões livres usam o mesmo controle e checkpoints após cada
subtração de retângulos, na fase Preflight. Validação da cena e filtragem/
ordenação final não são interrompíveis internamente. O parsing de argumentos
verifica cancelamento antes, durante uma passagem sem alocação em blocos de
16 KiB e depois do Serde. A chamada Serde limitada ainda é indivisível e pode
consumir até o teto configurado de 1 MiB antes do check posterior. Conversão
de resultados e outros diagnósticos ainda têm lacunas de cancelamento;
callbacks/observers devem retornar prontamente e não são interrompidos à força.

Preview/export (incluindo CSV) transmitem os bytes do exporter por scratch
base64 de 8 KiB para a String limitada do resultado, retendo no máximo dois
bytes brutos entre writes em vez do artefato bruto completo. Metadados, IDs de
tabela, escapes JSON e loss reports usam o mesmo orçamento, reconferido no
envelope completo. Isso não limita o scratch do exporter ou codec.

Resultados tipados de inspect/explain, preflight, debug-mask e regiões livres
são contados contra `max_result_bytes` antes da conversão para Value JSON.
Resultados excessivos param na contagem, sem construir a árvore JSON adicional.
A checagem final permanece ativa. Isso não limita a cena resolvida ou os DTOs
já construídos nem substitui a contabilização de artefatos/envelope base64.
A inspeção de página é serializada de uma visão emprestada da cena: nomes de
exclusions/regions e IDs em overflow são percorridos diretamente, portanto a
rejeição não clona primeiro essas listas em `PageInspection`. A saída aceita
mantém exatamente o formato JSON do core e possui as strings JSON finais.

**BETA PÚBLICA — `0.1.0-beta.2`.** APIs e comportamento podem mudar antes da
versão estável. Valide outputs, limites e tratamento de falhas para sua carga;
implementação e testes locais não equivalem a certificação de produção.

[English](README.en.md) | [Français](README.fr.md)

Bridge opcional e limitado entre `appcore-ai` e `appcore-filemaker`. Ele mantém
policy do modelo, schemas de tools, budgets de chamadas, validação de mutações e
acesso a artifacts fora do core determinístico do FileMaker.

Todos os argumentos usam schemas fechados, mutações resolvem um candidato antes
do commit e os limites do bridge só podem restringir `ResourceLimits` do core.
O tamanho serializado do resultado é escrito em um contador limitado que não
retém bytes e para em `max_result_bytes`, sem alocar um segundo JSON completo.

O ciclo completo create/patch/inspect/validate/preview/debug-mask/export é
executável e validado pela policy. Sessões de dataset podem exportar uma tabela
selecionada como CSV limitado em memória; tools gráficas ainda exigem cena
resolvida.
Discovery de capabilities e export expõem PDF editável, flattened e híbrido;
hybrid combina outlines vetoriais com texto Unicode invisível e pesquisável.
A descoberta de schema expõe escrita `horizontal` e `vertical_rl` implementada;
somente emoji colorido continua uma capability preparada.

Veja o [guia](wiki/guide.pt.md), o [exemplo básico](wiki/examples/basic.pt.md)
e o [exemplo intermediário](wiki/examples/intermediate.pt.md).

Licença: MIT.

## Documentação estável

ID estável: **ACR-024**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-024). Esse ID permanente
continua válido se a página da wiki mudar.
