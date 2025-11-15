# appcore-update

**Responsabilidade:** seleção, autenticidade, stage, ativação, health gate e
rollback de artefato opaco.

**Dependências internas:** contracts e provider.

**API principal:** artifact descriptor/signing payload; verifier,
unsigned-local protegido por feature/Ed25519, trust policy/key status; update request/provider e file
factory; staged artifact, activation receipt/store; coordinator,
preparation/outcome, health check e fault injection.

Use para binários ou artefatos opacos. O Runtime valida identidade, versão,
protocolo, checksum e trust, sem entender código ou schema.

Leituras de arquivo são limitadas e rejeitam componente final não regular. A
ativação revalida tamanho e SHA-256 do staged e cria hard link para um path de
build imutável. Um path existente só é reutilizado quando os bytes correspondem
exatamente ao descriptor; nunca é substituído. O no-follow atômico do
componente final existe em Unix. Outras plataformas mantêm checks de metadata,
mas dependem da fronteira do filesystem do deployment contra races de reparse.

**Maturidade:** lifecycle RC estável; supply chain remoto exige assinatura,
provenance e trust roots.
