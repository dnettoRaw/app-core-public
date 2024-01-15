# appcore-security

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

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

A RC 1.0 não possui provider TPM ou hardware-backed. O ADR 0005 registra uma
proposta aditiva para 1.1, com fallback explícito e evidência em hardware real;
o Runtime atual não alega proteção por hardware.

**Maturidade:** contratos RC estáveis; produção depende do backend de segredo e
controles do deployment.
