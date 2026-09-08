# appcore-capabilities

Testes locais:

```bash
cargo test -p appcore-capabilities
```

`CapabilityCatalog` aplica os mesmos limites de quantidade e bytes da versão.
`from_descriptors` para de consumir a entrada na primeira rejeição, sem
coletar tudo antes de validar.

O `CapabilityRegistry` local admite até 4.096 handlers e versões de descriptor
de 1–256 bytes UTF-8. Rejeições preservam handlers registrados e retornam
`HandlerRejected` com motivo limitado. `iter_descriptors` empresta descriptors
sem clones nem coleção alocada; a ordem não é especificada. Esses limites não
controlam memória interna de handlers externos nem do método descriptor.

**Responsabilidade:** catalogar descritores, registrar handlers locais e
resolver providers locais ou remotos compatíveis.

**Dependências internas:** contracts, core e distributed contracts.

**API principal:** catálogo e contexto de enforcement, request/response/error,
traits local handler e remote invoker, local provider, registry, provider
selection, resolution policy, selection trait/default, resolver e invoker peer
RPC baseado no contrato distribuído.

O catálogo valida descritores compostos do manifesto sem declarar um handler
fictício; o registry possui somente handlers executáveis. Catálogo e resolver
compartilham enforcement de mode, idempotência, escrita e liderança. O Runtime
não infere significado de produto a partir dos nomes de capabilities.

O resolver padrão percorre os records de discovery por empréstimo, guarda
somente o primeiro fallback compatível e clona apenas o provider selecionado.
Ele não materializa todos os peers compatíveis nem clona a lista completa de
capability names depois que o descriptor já correspondeu. Uma
`CapabilitySelectionPolicy` customizada continua recebendo o slice owned
completo exigido pelo contrato público estável.

Use `CapabilityResolver::handle_owned` quando o caller possui um request
selecionado para execução local ou remota. Handlers locais mantêm seu contrato
emprestado; o invoker Peer RPC move ID, capability, payload, chave de
idempotência e trace diretamente para o request de saída. `handle` e
`RemoteCapabilityInvoker::invoke_remote` permanecem compatíveis com callers e
implementações emprestados.

A execução default de `handle`, `handle_local` e `handle_owned` também empresta
o provider do registry ou record de discovery selecionado durante enforcement
e dispatch. `resolve()` continua deliberadamente retornando um
`CapabilityProvider` owned, e selectors customizados continuam recebendo sua
lista owned completa de candidatos.

**Maturidade:** perfil de roteamento RC estável.

## Documentação estável

ID estável: **ACR-016**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-016). Esse ID permanente
continua válido se a página da wiki mudar.
