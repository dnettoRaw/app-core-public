# appcore-sdk

O benchmark `runtime` mede construção com defaults, preparação vazia e
preparação de 32 comandos. Execute `cargo bench -p appcore-sdk --bench runtime`.
`APPCORE_BENCH_CASE` seleciona `construct_defaults`, `prepare_empty` ou
`prepare_32_commands`. Mede construção/validação/destruição da facade, não
startup HTTP ou cluster. `--features full` muda o perfil compilado, mas não
exercita todas as capabilities opcionais.

[English](README.en.md) | [Français](README.fr.md)

`appcore-sdk` é a fachada documentada para aplicações AppCore. Ela mantém a
dependência inicial pequena e deixa infraestrutura nos crates que a possuem.
Ele substitui o `appcore-bin`, agora aposentado, sem preservar o antigo host ou
CLI do Runtime.

```rust
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        app.log("Olá, mundo!");
        Ok(())
    })
}
```

Implemente `Application` para comandos, queries e tarefas no deployment
explícito. A fachada base requer apenas contratos, o Runtime core e logging
limitado. `App::prepare` executa todos os hooks de registro e retorna registries
imutáveis do Core, um router de queries congelado e tarefas limitadas sem
iniciar um host. Ative `api`
para queries HTTP, `scheduler` para tarefas agendadas, `deployment` para
bindings resolvidos por provider, ou `storage`, `sync`, `ai` e `filemaker`
apenas quando essas capabilities forem necessárias. Providers, listeners e
ciclo de processo continuam responsabilidades da aplicação/deployment.

## Exemplos executáveis

Os exemplos ficam organizados por progressão em `examples/`: `01-basics`,
`02-manifests`, `03-application`, `04-storage`, `05-sync`, `06-ai` e
`07-filemaker`, `08-logging` e `09-api`. Execute com
`cargo run -p appcore-sdk --example NOME`; os casos
de capability exigem a feature correspondente.

Use `App::logging(LoggerConfig)` para selecionar terminal, arquivo JSONL
limitado, ambos, desligado ou somente crash. Nome, bytes por arquivo, rotações
ativas e o arquivo opcional em `YYYY/MM` são explícitos. O modo somente crash é
gravado com `App::dump_crash_log` e não cria arquivo durante a execução normal.
Veja `08-configured-logging`.

## Documentação estável

ID estável: **ACR-028**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-028). Esse ID permanente
continua válido se a página da wiki mudar.

Aplicações existentes devem seguir o
[guia de migração](wiki/migration.pt.md) mantido pelo crate.
