# appcore-capabilities

**Responsabilidade:** catalogar descritores, registrar handlers locais e
resolver providers locais ou remotos compatíveis.

**Dependências internas:** contracts, core e distributed contracts.

**API principal:** catálogo e contexto de enforcement, request/response/error,
traits local handler e remote invoker, local provider, registry, provider
selection, resolution policy, selection trait/default, resolver e invoker peer
RPC baseado no contrato distribuído.

O catálogo valida descritores compostos do manifesto sem declarar um handler
fictício; o registry possui somente handlers executáveis. Catálogo e resolver
compartilham enforcement de mode, idempotência, escrita e liderança.

**Maturidade:** perfil de roteamento RC estável.
