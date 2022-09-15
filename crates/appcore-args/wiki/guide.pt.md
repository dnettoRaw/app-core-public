# Guia Do appcore-args

Autor: [dnettoRaw](https://github.com/dnettoRaw)

[English](guide.en.md) | [Français](guide.fr.md) |
[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

## Responsabilidade

O crate possui especificações de comandos, leitura limitada de argumentos,
parsing determinístico, ajuda gerada, candidatos de completion e integração
dinâmica com shells. Os consumidores possuem a execução e todo comportamento
do Runtime.

Este é um crate autônomo com versão independente. Sua API pública não pode
importar contratos ou tipos de qualquer outro crate AppCore.

## Modelo De Comandos

- `CliSpec` e `CommandSpec` definem comandos aninhados, aliases, opções
  herdadas, posicionais e subcomandos obrigatórios.
- `OptionSpec` define nomes longos e curtos, valores proibidos, obrigatórios ou
  opcionais, repetição, requisitos e conflitos.
- Opções terminais como `--help` podem ignorar validações de entradas
  obrigatórias.
- `ArgumentSpec` define posicionais fixos ou um último posicional variável.
- `ValueType` valida texto, booleanos e inteiros com ou sem sinal.

Toda especificação é validada antes de parsing, ajuda ou completion. Nomes
inválidos, aliases duplicados, colisões de opções herdadas, relações
desconhecidas e layouts posicionais ambíguos falham de forma fechada.

## Fronteira De Entrada

`RawArgs::from_env` rejeita entradas não UTF-8 sem conversão com perda. Os
limites padrão são 1.024 palavras, 64 KiB por palavra e 1 MiB no total. Limites
customizados estão disponíveis em `RawArgs::parse_with_limits`. Bytes NUL são
sempre rejeitados.

O parser aceita `--name value`, `--name=value`, flags agrupadas como `-av`,
valores curtos anexados como `-oresult` ou `-o=result`, posicionais negativos
com sinal e passthrough após `--`. Valores opcionais aceitam apenas
`--name=value` ou valor curto anexado, evitando consumir o próximo posicional
de forma ambígua. O consumidor pode habilitar um valor opcional separado com
`detached_optional_value(true)`; use um tipo restritivo como `Bool` para
consumir apenas a próxima palavra válida.

Comandos, opções longas e valores enumerados desconhecidos incluem uma sugestão
próxima quando ela existe. O cálculo é limitado a entradas e candidatos de 128
bytes; valores maiores ainda retornam o erro tipado sem análise de similaridade.

## Ajuda E Completion

`HelpRenderer` e `CompletionEngine` consomem a mesma especificação validada do
parser. Entradas ocultas são omitidas, opções não repetíveis já usadas não são
sugeridas e valores possíveis viram candidatos de completion.

`render_dynamic_completion_script` suporta Bash, Zsh, Fish e PowerShell. Os
tokens do executável e do comando de completion são restritos antes da
interpolação. Sem candidato estrutural, as integrações preservam o completion
nativo de arquivos do shell.
