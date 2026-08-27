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

## Keyring Windows DPAPI opcional (`1.0.2-rc`)

No Windows, selecione o secret provider `windows-dpapi-user-v1`, configure seu
`root` e defina `runtime_security = "provider:active"`. Inicialize e rotacione
o mesmo provider explícito com `appcore-bin security secret
keyring-init|keyring-rotate --keyring PATH --keyring-provider
windows-dpapi-user-v1`. O escopo do usuário atual e da máquina atual nunca faz
fallback para o file keyring legado. A certificação real Windows do AC-009
continua pendente; o comportamento estável 1.0 não muda.

## AI alpha opcional

A feature `ai-alpha` anexa um `appcore_ai::AiRuntime` já configurado ao
Supervisor existente sem alterar os manifests V1 congelados:

```rust
let component = Arc::new(AppCoreAiComponent::new(Arc::new(ai_runtime), false)?);
let ai = component.facade();
let business = MinhaAplicacao::new(ai);
ManifestApplicationHost::load("application.toml", "deployment.toml", &business)?
    .with_ai(component)
    .run()?;
```

`required = true` falha o startup quando não existe modelo/backend utilizável;
`false` inicia degradado. O shutdown bloqueia novas admissões, cancela requests
ativas e respeita o prazo limitado do Supervisor. Expor
`appcore.ai.resolve` por `appcore-capabilities` exige um `AiCapabilityCodec`
limitado e pertencente à aplicação; os tipos Rust não viram wire format
implicitamente. A seleção declarativa exige um futuro contrato de manifesto
versionado pós-1.0.

Tanto `appcore-bin` quanto `appcore-auth-server` usam a fronteira limitada de
`appcore-args`. A ajuda e os candidatos de completion vêm da mesma
especificação de comandos validada.

Os descritores finais de capability do manifesto são compostos uma vez por
`appcore-capabilities`. Facade direta, HTTP de aplicação e peer RPC usam esse
owner para enforcement de mode, idempotência, modo de escrita e liderança.

Handlers de comando pela facade direta, HTTP de aplicação e peer RPC executam
sem manter o mutex compartilhado do host. Comandos independentes avançam em
paralelo; reserva e finalização idempotentes permanecem serializadas por store.
O shutdown interrompe novas admissões, drena por até 30 segundos os comandos já
admitidos e conclui o lifecycle. Testes podem escolher um limite menor com
`ManifestApplicationHost::shutdown_with_timeout`.
O registro de queries de aplicação é congelado após o bootstrap; queries
diretas, HTTP e peer RPC clonam o router imutável e executam sem o mutex do host.

Na versão candidata `1.0.2-rc`, o serviço HTTP selecionado usa uma geração de
`ReloadableRuntimeHttpHost` sob o Supervisor já existente. Isso não ativa
polling de manifest nem altera rotas estáveis. O boundary de mesmo listener já
prepara, troca, drena e faz rollback; mudança de endereço ainda exige suporte
explícito da composition root.

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
