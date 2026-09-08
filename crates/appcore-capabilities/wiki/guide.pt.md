# appcore-capabilities

`CapabilityCatalog` aplica os mesmos limites de quantidade e bytes da versão.
`from_descriptors` para de consumir a entrada na primeira rejeição, sem
coletar tudo antes de validar.

O `CapabilityRegistry` local admite até 4.096 handlers e versões de descriptor
de 1–256 bytes UTF-8. Rejeições preservam handlers registrados e retornam
`HandlerRejected` com motivo limitado. `iter_descriptors` empresta descriptors
sem clones nem coleção alocada; a ordem não é especificada. Esses limites não
controlam memória interna de handlers externos nem do método descriptor.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** catalogar descritores compostos, registrar handlers
locais e resolver providers locais ou remotos compatíveis.

**Dependências internas:** contracts, core e distributed contracts.

**API principal:** request/response/error, traits local handler e remote
invoker, catálogo e contexto de enforcement, local provider, registry, provider
selection, resolution policy, selection trait/default, resolver e invoker peer
RPC baseado no contrato distribuído.

Use IDs genéricos e requisitos explícitos. O resolver considera health, mode,
liderança e policy; não interpreta semântica de produto.

Use `CapabilityCatalog` quando a composition root precisar resolver e autorizar
descritores do manifesto antes do dispatch. Use `CapabilityRegistry` apenas
quando houver um handler local real. Catálogo e resolver compartilham
enforcement de request, modo de escrita e liderança.

A seleção padrão limita alocações ao resultado escolhido: o discovery é
percorrido por referências emprestadas de peer e descriptor, somente o
primeiro fallback compatível é guardado e apenas o provider selecionado é
clonado. A compatibilidade usa o descriptor que já correspondeu em vez de
percorrer uma cópia de todos os nomes anunciados. Uma
`CapabilitySelectionPolicy` customizada mantém o comportamento estável e
recebe o slice owned completo.

Quando o caller não precisa mais do request, use
`CapabilityResolver::handle_owned`. Resolução e policy continuam emprestando o
request; um handler local selecionado mantém o contrato borrowed estável,
enquanto o invoker Peer RPC transfere todos os campos owned para seu DTO de
saída. `RemoteCapabilityInvoker::invoke_remote_owned` possui um default
borrowed, portanto invokers customizados existentes continuam compatíveis.

Os três métodos de execução emprestam o provider local ou record de discovery
selecionado pelo default até o dispatch terminar. Isso evita clonar identidade,
endpoints, capabilities e metadata de um peer para uma chamada transitória.
`resolve()` ainda retorna um provider owned, enquanto um selector customizado
mantém o contrato da lista completa de candidatos owned.

**Maturidade:** perfil de roteamento RC estável.
