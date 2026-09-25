# appcore-security

Os limites bearer V1 estão em `appcore_security::token`: claims JSON decodificadas
até 64 KiB, assinatura do provider até 256 KiB e envelope hexadecimal até
655.364 bytes. Ambos os componentes são conferidos antes de decodificar ou
verificar a assinatura; excesso retorna `CommandTokenError::InvalidFormat`.
A emissão limita campos e escapes JSON antes de assinar e rejeita saída vazia
ou excessiva do provider. São limites de segurança, não um novo formato wire;
tokens anteriormente excessivos precisam ser reemitidos com claims menores.
Alocações internas do provider e buffers do chamador não são controlados por
esta fronteira. O HTTP pode impor limite menor. Tokens não devem transportar
payloads da aplicação.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** contratos reutilizáveis de autenticação, token, segredo e
policy.

**Dependências internas:** `appcore-core`, `appcore-dnt`.

Use `create_private_directory` ao criar um diretório sensível e
`open_private_directory` quando ele já deve existir. O guard valida o diretório
final e seus ancestrais, rejeita componentes symlink/reparse, mantém handles
presos durante a operação e falha fechado quando a plataforma não oferece os
controles necessários. Só aplica permissões owner-only a diretórios novos;
permissões ou ACLs inseguras existentes são rejeitadas.
Use `PrivateDirectoryGuard::join` para paths filhos. Ele rejeita paths
absolutos, traversal com `.`/`..` e caracteres de controle antes que outro
crate abra o path derivado.

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

`RequestValidationDetailsRef` e `RequestPayloadRef` oferecem um caminho aditivo
emprestado para requests em trânsito. `compute_borrowed_request_hash` preserva
exatamente a saída V2 enquanto conta e hasheia JSON estruturado diretamente em
duas passagens, sem reter um payload codificado completo. O contrato owned
continua disponível por compatibilidade.

`CommandTokenValidator` rejeita centralmente emissão no futuro, ordem temporal
inválida e duração das claims acima de `TokenClaims::ttl_ms`. Callers que
coordenam relógios distintos podem aceitar no máximo cinco minutos de skew
positivo na emissão com `with_clock_skew_ms`; a expiração continua estrita.

## Provider Windows DPAPI no `1.0.2-rc`

`WindowsDpapiSecretKeyring` protege cada registro limitado com DPAPI não
interativo no escopo do usuário atual e da máquina atual. O keyring também
exige DACL protegida exclusiva do proprietário, rejeita symlinks, junctions e
outros reparse points e zera os owners de plaintext. Selecione
`windows-dpapi-user-v1` explicitamente; um diretório `file-keyring-v1` existente
é rejeitado pelo marcador de formato, sem conversão nem fallback.

O mesmo usuário na mesma máquina pode restaurar um backup completo do
diretório após descriptografar e validar todos os registros. Outro usuário ou
outra máquina deve falhar fechado. A certificação real multiusuário e
multimáquina continua pendente no AC-009; o RC é evidência de preview da
implementação, não certificação de produção. O comportamento estável 1.0 não
muda e a atualização é explícita.

**Maturidade:** contratos RC estáveis; produção depende do backend de segredo e
controles do deployment.
