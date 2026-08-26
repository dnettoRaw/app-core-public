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

Handlers de comando acessados pela facade direta, HTTP de aplicação ou peer RPC
executam sem manter o mutex compartilhado do host. Comandos independentes podem
avançar em paralelo; reserva e finalização idempotentes permanecem serializadas
por store. `shutdown()` fecha a admissão e drena comandos admitidos por no
máximo 30 segundos. `shutdown_with_timeout` expõe um prazo limitado menor para
testes e hosts embutidos.
O registro de queries de aplicação é congelado após o bootstrap; queries
diretas, HTTP e peer RPC clonam o router imutável e executam sem o mutex do host.

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

## AI alpha opcional

Ative `appcore-bin/ai-alpha`, construa um `appcore_ai::AiRuntime` com limites,
admission, registro de modelos e backends explícitos, e envolva-o em
`AppCoreAiComponent`. Injete `component.facade()` no código de negócio antes de
carregar o host e conclua a composição com
`ManifestApplicationHost::with_ai(component)`. O Supervisor existente possui
startup, saúde required/optional, cancelamento e shutdown limitado.

Essa feature é programática porque os manifests V1 estão congelados. Ela não
infere providers, baixa modelos nem define payload de wire. Registrar o handler
local `appcore.ai.resolve` exige um `AiCapabilityCodec` limitado e pertencente à
aplicação. Consulte os exemplos executáveis lightweight, OpenAI-compatible e
Candle de `appcore-ai` para construir o runtime.

**Maturidade:** facade manifest-first RC estável; a integração AI é um opt-in
`0.1.0-alpha` separado e internals são detalhes.
