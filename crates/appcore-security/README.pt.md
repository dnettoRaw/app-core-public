# appcore-security

**Responsabilidade:** contratos reutilizáveis de autenticação, token, segredo e
policy.

**Dependências internas:** `appcore-core`, `appcore-dnt`.

**API principal:** provider HashToken, claims, factory/validator de command
token, request hash, `SecurityError`; referências, resolvers, stores, bytes
zerados, file keyring, metadata/rotação, contrato Vault, peer credentials,
adapter de key provider DNT, traits de autenticação e policy.

Use para autenticação de infraestrutura e indireção de segredo. Tokens são
assinados, não criptografados. Não coloque autorização de domínio, OAuth,
inbound TLS ou vault gerenciado aqui.

`HashTokenProvider::from_secret`, `with_secret` e `with_material` retornam
`SecurityResult` e aplicam as mesmas invariantes mínimas de secret e salts.
`compute_request_hash` produz um SHA-256 com marcador `v2:` sobre campos
separados por domínio, com tamanho e presença de opcionais explícitos. Hashes
anteriores sem versão são rejeitados; emissores e validadores devem ser
atualizados juntos.

O alpha 1.5 adiciona `WindowsDpapiSecretKeyring`, disponível somente no Windows.
Os registros ficam protegidos para o usuário atual na máquina atual, mantêm ACL
exclusiva do proprietário e rejeitam reparse points. A composição seleciona
explicitamente `windows-dpapi-user-v1` com `provider:active`; nunca há fallback
para o file keyring nem para DPAPI de escopo da máquina. A certificação real
multiusuário e multimáquina continua pendente no AC-009, portanto este
pré-release ainda não é uma alegação de certificação para produção.

A linha estável 1.0 não possui provider TPM, DPAPI ou hardware-backed. A
seleção do prerelease 1.5 é explícita e não muda o comportamento do keyring
legado.

**Maturidade:** contratos RC estáveis; produção depende do backend de segredo e
controles do deployment.
