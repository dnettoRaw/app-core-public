# appcore-bin

**Responsabilidade:** facade manifest-first, CLI e composition root.

**Dependências internas:** todos os crates de serviço/composição.

**API de aplicação:** `Application`, `run_application`,
`ManifestApplicationHost`, `ApplicationServiceReport`, `DeploymentContext`,
volumes/environment resolvidos e `ApplicationTaskRegistry`.

**API de host:** bootstrap/config errors/results, CLI, paths/lifecycle local,
server entry points, build info e ferramentas opcionais de auth-server.

É a dependência recomendada para aplicações. Possui carregamento de manifests,
providers, lifecycle, HTTP, sync, peer RPC, control plane, Gateway, scheduling,
supervision, updates e shutdown.

Aplicações usam o módulo público `application` e evitam internals.

Tanto `appcore-bin` quanto `appcore-auth-server` usam a fronteira limitada de
`appcore-args`. A ajuda e os candidatos de completion vêm da mesma
especificação de comandos validada.

Os descritores finais de capability do manifesto são compostos uma vez por
`appcore-capabilities`. Facade direta, HTTP de aplicação e peer RPC usam esse
owner para enforcement de mode, idempotência, modo de escrita e liderança.

Quando `deployment.toml` seleciona `[adapters.gateway]` com o provider
`appcore-gateway`, o bootstrap valida a configuracao do owner, inclui e
autoriza `runtime.gateway` nesse catalogo, reutiliza a seguranca do Runtime e
registra a instancia no Supervisor. Falha de bind ou configuracao aborta o
startup. `ApplicationServiceReport` expoe started/state/bind do Gateway sem
credenciais. O host fornece replay store duravel e seguro entre processos;
cluster exige `paths.gateway_replay` absoluto em volume compartilhado e gravavel. O
shutdown fecha conexoes incompletas antes do prazo e faz join do listener e da
thread de runtime.

```bash
appcore-bin completions zsh
appcore-auth-server completions powershell
```

**Maturidade:** facade manifest-first RC estável; internals são detalhes.
