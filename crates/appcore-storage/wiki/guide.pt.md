# appcore-storage

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** contratos genéricos de storage e provider local em
arquivo.

**Dependências internas:** `appcore-contracts`, `appcore-dnt`,
`appcore-security`, `appcore-types`.

**API principal:** `StorageProvider`, `Repository`, `Migration`, `Transaction`,
health/status/errors, IDs validados, `FileStorageProvider`, manifests de
storage, backup V1, helpers autenticados de storage remoto e stores opcionais
selados por DNT para objetos, snapshots e segredos.

O auth-storage remoto V1 separa suas representações limitadas: plaintext de
até 256 KiB para `seal`, dados selados de 384 KiB para `open`, body de token
autenticado de 1 MiB e headers HTTP de 64 KiB. Input acima do teto falha antes
da expansão hex/JSON; o parser do cliente reutiliza o buffer owned da resposta.

O adapter selado em arquivo escreve DNT normal por padrão e expõe
`DntFileObjectStore::write_object_compact` para snapshots, backups e arquivos
de domínio exportáveis quando o payload for compressível. Escritas compactadas
continuam sendo envelopes DNT comuns sobre o mesmo provider de arquivo; o
contrato do backend de storage não muda.
Leituras seladas derivam o limite do envelope completo de
`SealedStoragePolicy` e rejeitam arquivos grandes demais antes de alocar o
buffer do arquivo.

`FileStorageProvider::read_bytes` materializa no máximo 64 MiB e continua lendo
por `max + 1` depois de consultar metadata, portanto crescimento concorrente
não contorna o limite. O backup de arquivo único transmite no máximo 1 GiB para
um temporário exclusivo, sincroniza e faz rename atômico; uma falha remove o
temporário e mantém o destino anterior. Um snapshot completo aceita no máximo
1 GiB por arquivo e 16 GiB no total. Os limites são exportados como
`DEFAULT_FILE_READ_MAX_BYTES`, `MAX_STORAGE_BACKUP_FILE_BYTES` e
`MAX_STORAGE_SNAPSHOT_BYTES`.

O manifest do snapshot completo tem teto de 16 MiB. Seu pretty JSON V1 é
serializado diretamente por um writer limitado de 16 KiB para um temporário
atômico exclusivo e desserializado por um reader limitado de 16 KiB. O buffer
codificado completo não coexiste mais com o inventário de arquivos decodificado;
input exatamente no limite continua válido e um byte não retido detecta
crescimento.

Use quando aplicação ou serviço precisa do perfil local-first documentado.
Mantenha schemas e tabelas de domínio fora. Transações não suportadas falham.

`StorageWriteBarrier` coordena writers com a instalação de update: obtenha um
permit `open`, chame `block_new_writers` e depois `drain` com deadline. Use
`release` após um drain bem-sucedido ou `seal_after_install_start` quando a
instalação começar. Permits nested e snapshots limitados de owners são
suportados; o estado selado nunca é limpo automaticamente no processo.

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

No preflight pós-1.0 explícito, `StorageCapabilityDescriptorV1` usa sete
garantias fechadas e catálogo limitado a 32 providers. O deployment lista
requisitos exatos no setting `required_capabilities` do provider de storage.
O requisito existente `storage.shared=true` adiciona `multi_host`. Requisitos
desconhecidos, duplicados, indisponíveis ou não suportados retornam erros
tipados e redigidos antes de abrir storage; não existe fallback. O descriptor
de arquivo fornece somente `snapshot`.

[Evidência clean-source do preflight](benchmarks/storage-capability-v1-2026-08-26.pt.md)

**Maturidade:** contratos RC estáveis; provider em arquivo certificado para um
processo local e filesystem com locks/sync/rename adequados.
