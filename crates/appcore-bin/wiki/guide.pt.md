# appcore-bin

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** facade manifest-first, CLI e composition root.

**Dependências internas:** todos os crates de serviço/composição.

**API de aplicação:** `Application`, `run_application`,
`ManifestApplicationHost`, `ApplicationServiceReport`, `DeploymentContext`,
volumes/environment resolvidos e `ApplicationTaskRegistry`.

**API de host:** bootstrap/config errors/results, CLI, paths/lifecycle local,
server entry points, build info e ferramentas opcionais de auth-server.

Os dois binários processam entrada UTF-8 limitada por `appcore-args`. Ajuda,
validação e completion dinâmica para Bash, Zsh, Fish e PowerShell compartilham
uma especificação declarativa; a execução permanece neste crate.

O manifesto distribuído final alimenta um único catálogo de
`appcore-capabilities` durante o bootstrap. Facade direta, HTTP de aplicação e
peer RPC usam o mesmo owner para enforcement de declaração, mode,
idempotência, escrita operacional e liderança. Queries de status do Runtime
permanecem comportamento explícito do host.

Selecionar `[adapters.gateway]` com provider `appcore-gateway` e a fronteira
declarativa de ativacao do Gateway. O bootstrap faz parse pela crate owner,
inclui e autoriza `runtime.gateway` no catalogo compartilhado, reutiliza a
seguranca do Runtime e registra o servico no Supervisor. Falha de configuracao
ou bind aborta o startup; a ausencia nao cria listener nem task de Gateway.
`ApplicationServiceReport` expoe started, state e bind seguros, e o shutdown
do host faz join de todo o trabalho possuido pelo Gateway. O replay store e
seguro entre processos; cluster exige `paths.gateway_replay` absoluto em volume
compartilhado e gravavel. O shutdown fecha conexoes incompletas antes do prazo.

É a dependência recomendada para aplicações. Possui carregamento de manifests,
providers, lifecycle, HTTP, sync, peer RPC, control plane, Gateway, scheduling,
supervision, updates e shutdown.

Aplicações usam o módulo público `application` e evitam internals.

**Maturidade:** facade manifest-first RC estável; internals são detalhes.
