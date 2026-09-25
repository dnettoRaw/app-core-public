# Guia do AppCore SDK

O benchmark `runtime` mede construção com defaults, preparação vazia e
preparação de 32 hooks Core: comandos, eventos, estados, declarações de decisão,
nós executáveis de decisão e handlers. Execute `cargo bench -p appcore-sdk --bench runtime`.
`APPCORE_BENCH_CASE` seleciona `construct_defaults`, `prepare_empty` ou
`prepare_32_core_hooks`. Mede construção, validação e destruição da facade, não
startup HTTP ou cluster. Queries e tasks dependem de features e não fazem parte
deste caso Core. `--all-features` adiciona `prepare_32_full_hooks`, que também
prepara 32 queries congeladas e 32 definições limitadas de tasks. Nenhum backend
opcional é iniciado.

`appcore-sdk` é a fachada para código de aplicação. Comece com `App` e `run`;
adicione manifestos explícitos quando precisar definir identidade/versão local
ou política de deployment.

1. Adicione `appcore-sdk` às dependências da aplicação.
2. Chame `appcore_sdk::run` com um nome estável para a aplicação.
3. Use o `App` recebido somente para log local e acesso a manifestos validados.
4. Implemente `Application` para comandos e eventos e chame `App::prepare` com
   a identidade explícita do node.
5. Adicione `api` para consultas HTTP, `scheduler` para tarefas agendadas e
   `deployment` somente quando a aplicação consumir bindings resolvidos por provider.
6. Faça o deployment com um processo Runtime explícito.

O SDK não inicia listener, não grava dados, não resolve segredos e não ativa
providers implicitamente. Os manifestos padrão são contratos V1 standalone,
não outra linguagem de configuração.

`App::prepare` é a ponte de lifecycle anterior ao hosting. Ele executa os hooks
uma vez, monta registries imutáveis do Core, congela o router de queries e
coleta tarefas limitadas do scheduler. O deployment consome esses valores e
continua responsável por providers, workers, cancellation e shutdown. Com a
feature `deployment`, ele chama `Application::configure` somente depois de
resolver e validar os bindings da instalação.

## Defaults e manifestos explícitos

Um programa local mínimo precisa de uma dependência AppCore direta,
`appcore-sdk`, e nenhum arquivo de manifesto. `App::new` ainda constrói
valores validados `ApplicationManifestV1` e `DeploymentManifestV1`. O
deployment standalone declara storage `file` e transports `http`, mas não
os inicia.

1. Construa `App` com a identidade estável da aplicação.
2. Opcionalmente use `App::application_manifest` para substituir todo o
   contrato da aplicação, sem mesclar campos selecionados com defaults.
3. Opcionalmente use `App::deployment_manifest` para substituir toda a política
   da instalação. Providers/rede explícitos não herdam defaults do SDK.
4. Ambos os manifestos devem corresponder à identidade do `App`. Contratos
   inválidos ou identidades diferentes retornam erro; defaults não ocultam falhas.
5. Consulte `effective_application_manifest()` e
   `effective_deployment_manifest()`, depois chame `run` ou `prepare`.

Manifestos explícitos também servem para mudar versão, serviço e identidade
da instalação local; não exigem serviços hospedados. A integração de deployment
é dona da leitura dos arquivos e resolução dos providers. O SDK não descobre
nem carrega arquivos de manifesto automaticamente.

| Necessidade | Superfície do SDK |
|---|---|
| Callback local e log | `run`, `App`, `AppResult`, `App::logger` |
| Comportamento registrado | `Application`, `NodeId` explícito, `PreparedApplication` |
| Consultas / tarefas | Features `api` / `scheduler` e registries preparados |
| Storage / replicação | Namespaces e features `storage` / `sync` |
| AI / documentos | Namespaces e features `ai` / `filemaker` |
| Bindings resolvidos da instalação | Feature `deployment`; `prepare_with_deployment` |
| Ciclo de vida do processo | Deployment da aplicação, não um host do SDK |

Ativar uma feature expõe APIs, não serviços em execução. Identidade do processo,
providers, cancelamento e shutdown continuam responsabilidades explícitas do deployment.

Veja [básico](examples/basic.pt.md) e [intermediário](examples/intermediate.pt.md).
O catálogo executável segue a mesma progressão: `01-basics`, `02-manifests`,
`03-application`, `04-storage`, `05-sync`, `06-ai` e `07-filemaker`.
`08-logging` e `09-api` completam o catálogo voltado à aplicação.

O SDK padrão não depende de API, provider ou scheduler. `storage`, `sync`,
`ai` e `filemaker` também são opt-in; use `full` somente em um consumidor de
desenvolvimento que realmente precise de todas as capabilities do SDK.

O log é configurado uma vez com `App::logging(LoggerConfig)`. Escolha terminal,
arquivo, ambos, desligado ou somente crash. Nome, tamanho, rotações próximas e
arquivo histórico opcional e limitado em `YYYY/MM` continuam explícitos. O
modo somente crash grava o diagnóstico limitado em memória por
`App::dump_crash_log`.

Use `DiagnosticBundleBuilder` para um relatório comum de suporte. Informe
somente um fingerprint não reversível do storage e observações limitadas dos
componentes. O builder redige texto livre, limita erros recentes, exclui
segredos e caminhos reais e registra flags de privacidade. O opt-in sensível é
explícito, mas o bundle V1 continua sem valores secretos.

Crie um `EnvironmentProfile` na fronteira do deployment e passe seu
`diagnostic_label()` para os diagnósticos. O perfil explicita estágio, track de
release, surface, topologia, labels de cluster/tenant, namespace de storage e
canal de update. Valide os namespaces não-release e release como um par; o SDK
não infere o ambiente por caminhos ou segredos.

Declare capabilities na fronteira da aplicação e mapeie cada comando mutável
por `CapabilityRegistry`. Execute `coverage` durante a preparação para que
mapeamentos ausentes sejam visíveis. Converta decisões de autorização do host
em `CapabilityOutcome`; assim os erros de autenticação e permissão são
uniformes sem mover a autorização de negócio para o SDK.
