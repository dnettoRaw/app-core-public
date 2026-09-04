# Guia do appcore-filemaker

Comece pelo
[guia YAML passo a passo](https://wiki.appcore.dnettoraw.com/pt/crates/appcore-filemaker-yaml).
Ele constrói um template V1 estrito de forma incremental e traz a referência
completa dos campos aceitos no topo e nos elementos. Mantenha
`appcore-filemaker schema --json` como fonte executável da versão instalada.

Depois compare o [exemplo básico](examples/basic.pt.md) e o
[exemplo intermediário](examples/intermediate.pt.md). A
[referência de arquitetura e contratos](architecture.pt.md) explica os limites
do engine.

As camadas de página são percorridas de forma lazy em cada página física; a
resolução por role não cria uma lista temporária de referências de elementos.
O planejamento de fluxo distribuído usa a mesma passagem sem alocação para
calcular o espaçamento dos filhos visíveis.
O fingerprint também ordena nomes de assets emprestados, sem clonar cada nome
na resolução determinística.

Registre bytes exatos de fonts e uma lista ordenada de fallback antes da
medição; a ordem entra no fingerprint e exporters incorporam as famílias
realmente escolhidas nos glyph runs resolvidos. Aplique patches de runtime no
binding, antes do layout, para que medição, colisão, paginação e export usem
geometria recalculada.
O JSON do fingerprint usa uma passagem de dimensionamento seguida de hashing
direto sob o budget agregado `max_output_bytes`. Ele preserva o framing V1
exato sem reter os bytes JSON canônicos.

Para japonês vertical ou layouts semelhantes, use
`text_options.writing_mode: vertical`. O engine quebra pelo limite de altura,
molda cada coluna de cima para baixo e avança as colunas da direita para a
esquerda. Mantenha `horizontal` (o padrão) para texto horizontal e BiDi.
