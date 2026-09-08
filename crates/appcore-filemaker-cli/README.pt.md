# appcore-filemaker-cli

As saídas de render, CSV e collision mask passam por um buffer de 64 KiB
para um arquivo temporário exclusivo. O CLI não retém toda a saída codificada
antes da publicação. Falhas de export/flush preservam o destino existente e
tentam remover o temporário. O scratch interno dos exporters continua sujeito
aos limites do core; isso não garante export sem alocações.

**BETA PÚBLICA — `0.1.0-beta.2`.** APIs e comportamento podem mudar antes da
versão estável. Valide outputs, limites e tratamento de falhas para sua carga;
implementação e testes locais não equivalem a certificação de produção.

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

## Documentação estável

ID estável: **ACR-025**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-025). Esse ID permanente
continua válido se a página da wiki mudar.
