# Exemplo básico

[English](basic.en.md) | [Français](basic.fr.md) | [Intermediário](intermediate.pt.md)

Execute `cargo run -p appcore-filemaker --example basic`. Ele cria um relatório
operacional A4 completo de uma página com título e responsável vindos dos dados,
desenhos vetoriais semânticos, indicador de progresso, sparkline cúbica e tabela
de primeira classe com estilo condicional e total numérico verificado. O SVG é
gravado em `target/filemaker-examples/basic.svg`.

O documento fica separado em
[`examples/basic.yml`](../../examples/basic.yml), e os dados tipados ficam em
[`examples/basic-data.json`](../../examples/basic-data.json); o runner Rust não
embute nenhum dos payloads no código-fonte. Ele registra explicitamente a Noto
Sans sob OFL incluída no exemplo antes do layout, portanto fonte do host, asset
de filesystem, rede e IA nunca são implícitos. Veja
[`examples/basic.rs`](../../examples/basic.rs). A ordem pública é:
`Compiler`, compilar uma vez, associar dados e patches, `LayoutEngine` e então
`ExportRequest`.
