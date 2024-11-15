# appcore-capabilities

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

**Maturidade:** perfil de roteamento RC estável.
