# appcore-filemaker-cli

[English](README.en.md) | [Français](README.fr.md)

Adapter de linha de comando limitado para `appcore-filemaker`. Ele oferece
schema, validação, preflight, inspeção, debug, mask e render atômico com output
JSON estável e exit codes tipados.
Stdout humano e pretty-JSON é dimensionado sob teto de 512 MiB e então escrito
por buffers fixos, sem reter uma segunda `String` completa do output.

A CLI aplica patches JSON de runtime repetíveis, configura uma ordem explícita
de fallback de fonts, consulta regiões livres e exporta datasets tabulares
limitados como CSV sem enviar as linhas pelo layout gráfico.
`render --format pdf --pdf-mode hybrid` grava outlines determinísticos e uma
camada Unicode invisível e subsetada para output pesquisável e selecionável.
`schema --json` reporta `horizontal` e `vertical_rl` como modos de escrita
implementados; somente emoji colorido continua uma capability preparada.

Documentos YAML e dados executáveis são arquivos separados em `examples/`; os
exemplos de comando não escondem templates dentro de código Rust ou shell.

Veja o [guia](wiki/guide.pt.md), o [exemplo básico](wiki/examples/basic.pt.md)
e o [exemplo intermediário](wiki/examples/intermediate.pt.md).

Licença: MIT.
