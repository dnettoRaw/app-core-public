# appcore-filemaker-ai

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
