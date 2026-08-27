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

## Provider Windows DPAPI no alpha 1.5

`WindowsDpapiSecretKeyring` protege cada registro limitado com DPAPI não
interativo no escopo do usuário atual e da máquina atual. O keyring também
exige DACL protegida exclusiva do proprietário, rejeita symlinks, junctions e
outros reparse points e zera os owners de plaintext. Selecione
`windows-dpapi-user-v1` explicitamente; um diretório `file-keyring-v1` existente
é rejeitado pelo marcador de formato, sem conversão nem fallback.

O mesmo usuário na mesma máquina pode restaurar um backup completo do
diretório após descriptografar e validar todos os registros. Outro usuário ou
outra máquina deve falhar fechado. A certificação real multiusuário e
multimáquina continua pendente no AC-009; o alpha 1.5 é evidência de preview da
implementação, não certificação de produção. O comportamento estável 1.0 não
muda e a atualização é explícita.

**Maturidade:** contratos RC estáveis; produção depende do backend de segredo e
controles do deployment.
