# appcore-security

Testes locais:

```bash
cargo test -p appcore-security
```

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

`RequestValidationDetailsRef` e `RequestPayloadRef` oferecem um caminho aditivo
emprestado para requests em trânsito. `compute_borrowed_request_hash` preserva
exatamente a saída V2 enquanto conta e hasheia JSON estruturado diretamente em
duas passagens, sem reter um payload codificado completo. O contrato owned
continua disponível por compatibilidade.

O `1.0.2-rc` adiciona `WindowsDpapiSecretKeyring`, disponível somente no Windows.
Os registros ficam protegidos para o usuário atual na máquina atual, mantêm ACL
exclusiva do proprietário e rejeitam reparse points. A composição seleciona
explicitamente `windows-dpapi-user-v1` com `provider:active`; nunca há fallback
para o file keyring nem para DPAPI de escopo da máquina. A certificação real
multiusuário e multimáquina continua pendente no AC-009, portanto este
pré-release ainda não é uma alegação de certificação para produção.

O pacote estável original `1.0.0` não possuía provider TPM, DPAPI ou
hardware-backed. A seleção do provider DPAPI aditivo em `1.0.2-rc` é explícita
e não muda o comportamento existente do file keyring.

**Maturidade:** contratos RC estáveis; produção depende do backend de segredo e
controles do deployment.

## Documentação estável

ID estável: **ACR-010**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-010). Esse ID permanente
continua válido se a página da wiki mudar.
