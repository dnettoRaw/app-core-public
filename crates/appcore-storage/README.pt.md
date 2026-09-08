# appcore-storage

Testes locais:

```bash
cargo test -p appcore-storage
```

**Responsabilidade:** contratos genéricos de storage e provider local em
arquivo.

**Dependências internas:** `appcore-contracts`, `appcore-dnt`,
`appcore-security`, `appcore-types`.

**API principal:** `StorageProvider`, `Repository`, `Migration`, `Transaction`,
health/status/errors, IDs validados, `FileStorageProvider`, manifests de
storage, backup V1, helpers autenticados de storage remoto e stores opcionais
selados por DNT para objetos, snapshots e segredos.

O auth-storage remoto V1 aceita no máximo 256 KiB de plaintext para `seal`,
384 KiB de dados selados para `open`, body de token autenticado de 1 MiB e
headers HTTP de 64 KiB. Os limites exportados rejeitam antes da expansão
hex/JSON; o roundtrip default máximo de 256 KiB é coberto de ponta a ponta.

O adapter selado em arquivo escreve DNT normal por padrão e expõe
`DntFileObjectStore::write_object_compact` para snapshots, backups e arquivos
de domínio exportáveis quando o payload for compressível. Escritas compactadas
continuam sendo envelopes DNT comuns sobre o mesmo provider de arquivo; o
contrato do backend de storage não muda.
Leituras seladas derivam o limite do envelope completo de
`SealedStoragePolicy`. Arquivos que já excedem esse limite na verificação de
metadados são rejeitados antes de alocar o buffer; crescimento durante a leitura
é limitado e rejeitado após a leitura.

`FileStorageProvider::read_bytes` materializa no máximo 64 MiB e rejeita um
arquivo maior ou que cresça durante a leitura. O backup de arquivo único
transmite para um temporário exclusivo, aceita no máximo 1 GiB, sincroniza
antes do rename atômico e preserva o backup anterior em caso de falha.
Snapshots completos aceitam no máximo 1 GiB por arquivo e 16 GiB no total. As
constantes exportadas definem esses limites.

O manifest do snapshot completo tem teto de 16 MiB. Seu pretty JSON V1 é
serializado diretamente por um writer limitado de 16 KiB para um temporário
atômico exclusivo e desserializado por um reader limitado de 16 KiB. O buffer
codificado completo não coexiste mais com o inventário de arquivos decodificado;
input exatamente no limite continua válido e um byte não retido detecta
crescimento.

Use quando aplicação ou serviço precisa do perfil local-first documentado.
Mantenha schemas e tabelas de domínio fora. Transações não suportadas falham.

Housekeeping e traversal de backup são iterativos, limitados e nunca seguem
symlinks ou reparse points do Windows. A listagem usa timestamps persistidos no
manifest do snapshot e só recorre aos metadados de criação/modificação para
backups simples em arquivo. A abertura final usa no-follow da plataforma e é
revalidada sob o lock do processo. O perfil de um processo ainda pressupõe uma
raiz protegida pelo proprietário: a troca maliciosa de um diretório ancestral
por outro processo da mesma conta durante a operação permanece fora desta
boundary portátil.

O traversal visita no máximo 200.000 entradas incrementalmente, retendo apenas
a pilha limitada de 16.384 diretórios e os resultados exigidos pelo consumidor.
O snapshot mantém seus paths ordenados necessários sem uma segunda lista global
de entradas; health retém somente um contador, cleanup apenas os temporários
correspondentes e a validação de symlink nenhuma entrada. O teto de profundidade
continua 128.
A verificação do snapshot também conta arquivos reais incrementalmente e
empresta o path anterior ao conferir a ordem; ela não cria um segundo inventário
de paths nem clona um path por entrada.

O contrato pós-1.0 `StorageCapabilityDescriptorV1` descreve transactions,
locking, snapshots, streaming, backup online, multi-process e multi-host sem
expor detalhes do provider. `required_capabilities` é um setting explícito do
deployment; requisito desconhecido, duplicado ou ausente falha antes do startup.
O provider de arquivo anuncia somente `snapshot`. Manifests V1 e deployments
V1 existentes não compartilhados não mudam.

**Maturidade:** contratos RC estáveis; provider em arquivo certificado para um
processo local e filesystem com locks/sync/rename adequados.

## Documentação estável

ID estável: **ACR-011**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-011). Esse ID permanente
continua válido se a página da wiki mudar.
