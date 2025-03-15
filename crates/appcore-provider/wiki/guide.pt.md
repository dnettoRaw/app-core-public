# appcore-provider

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** factories, registry, deployment plans, contratos de
coordenação/job e resolução de segredo independentes de implementação.

**Dependências internas:** `appcore-contracts`.

**API principal:** `ProviderRole`, `ProviderContext`, `ProviderFactory`,
`ProviderRegistry`, `DeploymentProviderPlan`, errors/results,
`ResolvedSecret`/`SecretProvider`; coordination schema V2, stores memória/file;
leases de recurso compartilhado com fencing; job spec/lease/completion/provider.

Leases em filesystem usam lock por recurso, arquivo de estado versionado e um
sidecar versionado de high-water epoch. O sidecar é persistido antes de publicar
a lease ativa e sobrevive a release, restart e aquisição interrompida, portanto
um epoch não é reutilizado. O epoch é fencing token apenas
para escritores que o validam antes de gravar. Filesystems compartilhados sem
lock, rename, sync de diretório ou coerência de cache confiáveis não oferecem
proteção forte contra split brain por este adapter sozinho.

Use para compor providers explícitos. Não registre fallback silencioso nem SDK
específico de provider neste crate.

**Maturidade:** superfície de composição RC estável; jobs distribuídos estão
fora do primeiro perfil 1.0.
