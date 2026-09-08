# Migração do appcore-bin

Troque a dependência e os imports da aplicação; não reproduza o host antigo.
A versão final de `appcore-bin` no registro é apenas um aviso de aposentadoria.
Código novo deve depender diretamente de `appcore-sdk`.

1. Dependa de `appcore-sdk` e ative apenas as capabilities usadas pelo app.
2. Importe `Application` e os contratos de registro de `appcore_sdk`.
3. Mantenha `application.toml`, `deployment.toml` e o código de negócio.
4. Use `App::prepare` para validar e coletar os registros de negócio.
5. Deixe o executável de deployment resolver providers, chamar
   `prepare_with_deployment`, iniciar workers e controlar o shutdown.

Não existe alias de compatibilidade para `appcore_bin`, CLI de Runtime no SDK
ou seleção implícita de provider. Operações removidas do host devem falhar na
fronteira do deployment, nunca ser inferidas pela fachada.
