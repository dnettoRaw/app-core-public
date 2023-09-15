# appcore-dnt

**Responsabilidade:** contratos e helpers do contêiner cifrado genérico DNT.

**Dependências internas:** `appcore-contracts`, `appcore-types`.

**API principal:** `seal`, `open`, `open_owned`, `inspect_header`, `verify`,
`write_atomic`, `read_verified`, `rekey`, `migrate_envelope`,
`DntKeyProvider`, `DntCodec`, `DntHeader`, `DntContext`, `DntCompression`,
`KeyId`, `ContentType`,
`CodecId`, `DntFlags`, `dnt_user_flag`, `dnt_compose_flags` e
`DNT_FLAG_PAYLOAD_DEFLATE`.

DNT é um envelope binário para bytes arbitrários. `.dnt`, `.dntj`, `.dntb` e
`.dnto` são apenas convenções; consumidores inspecionam o header autenticado.

Layout V1:

```text
header canônico
  magic
  envelope_version
  header_length
  flags
  algorithm
  schema_version
  created_at_ms
  stored payload_length
  nonce
  payload_hash
  public_metadata_length
  encrypted_metadata_length
  application_id
  tenant_id opcional
  content_type
  codec_id
  key_id
  public_metadata
ciphertext
  encrypted_metadata_length
  encrypted_metadata
  payload codificado armazenado
tag de autenticação
```

Todo o header é AAD da AEAD. A V1 usa XChaCha20-Poly1305 com chave de 256 bits
e nonce aleatório de 192 bits vindo do sistema operacional. Chaves são
resolvidas por `DntKeyProvider`; nunca ficam dentro do envelope.

## Por Que Usar DNT

DNT não foi feito para substituir todo arquivo. Ele vale quando bytes precisam
passar por providers de storage, backups, transporte de sync ou guarda local de
segredos sem perder propriedades de segurança.

Use DNT quando o arquivo precisa de:

- confidencialidade sem colocar a chave ao lado dos bytes cifrados;
- identidade autenticada de aplicação, tenant, tipo lógico, codec e chave;
- rejeição de aplicação errada, tenant errado ou tipo lógico errado antes de
  retornar plaintext;
- detecção de corrupção e adulteração no header e no payload;
- escrita atômica e leitura verificada;
- rotação explícita de chave com `rekey`;
- migração explícita de envelope com `migrate_envelope`;
- transporte opaco por storage, sync ou gateway sem eles entenderem o domínio.

Não use DNT só para economizar disco. JSON puro ou binário bruto é mais simples,
menor e mais rápido quando não há necessidade de cifragem, autenticação,
vinculação de contexto, rotação de chave ou migração versionada.

## Modo Compactado

DNT normal armazena a saída do codec diretamente antes da cifragem. DNT
compactado marca o flag autenticado `DNT_FLAG_PAYLOAD_DEFLATE` e armazena um
stream DEFLATE com wrapper zlib em nível balanceado antes da cifragem. Leitores
V1 conseguem inspecionar os dois modos; abrir envelopes compactados exige
`DntOpenOptions.max_payload_bytes` para limitar a expansão.

Para buffers completos lidos de arquivo, prefira `open_owned` ou
`read_verified`; eles descriptografam o envelope proprietário in-place. Use
`open` quando o chamador só possui uma slice emprestada.

`read_verified` exige `DntOpenOptions.max_payload_bytes` explícito e rejeita
arquivo grande demais antes de alocar o buffer completo. Metadados cifrados V1
são limitados a 64 KiB. `OpenedDnt::zeroize_plaintext` limpa plaintext e
metadados cifrados retornados assim que o chamador não precisar mais deles.

| Modo | Tamanho em disco | Caminho de leitura |
|---|---|---|
| Normal | Header + metadados cifrados + payload codificado + tag AEAD. O tamanho acompanha a saída do codec e tem menor custo de CPU. | Lê, autentica, descriptografa e depois decodifica o codec. É o caminho de CPU mais rápido para arquivos pequenos ou pouco compressíveis. |
| Compactado | Header + metadados cifrados + payload codificado comprimido + tag AEAD. JSON repetitivo, snapshots e logs costumam ficar muito menores; payloads já comprimidos ou aleatórios podem ficar iguais ou maiores. | Lê menos bytes do disco, autentica, descriptografa, infla DEFLATE e decodifica o codec. A inflação adiciona trabalho, mas menos ciphertext pode reduzir AEAD e digest o suficiente para melhorar a latência total em payloads muito compressíveis. |

O modo compactado não deve ser tratado como barreira de segurança. O tamanho do
arquivo ainda revela uma aproximação do tamanho comprimido. Evite compactar
segredos que misturam bytes controlados por atacante com bytes confidenciais
quando a observação de tamanho importa.

### Comparativo De Referência

O repositório inclui um comparador reproduzível que grava cada amostra como
arquivo plaintext, DNT normal e DNT compactado. Ele aquece cada caminho e
reporta separadamente distribuições de espaço, read/open, seal e rekey:

```bash
cargo run -p appcore-dnt --example compare --release
```

Execução `--release` de referência em Apple M1, separada por categoria:

Espaço em disco:

- JSON repetitivo: plaintext 1.048.557 bytes; normal 1.048.746; compactado 4.403;
- binário incompressível: plaintext 1.048.576 bytes; normal 1.048.773;
  compactado 1.048.949;
- segredo pequeno: plaintext 65 bytes; normal 252; compactado 254.

Mediana do caminho de leitura com cache aquecido:

- JSON repetitivo: plaintext 42,7 us; read/open normal 5,51 ms; compactado
  321,2 us;
- binário incompressível: plaintext 42,3 us; normal 5,51 ms; compactado 6,33 ms;
- segredo pequeno: plaintext 14,5 us; normal 17,7 us; compactado 23,8 us.

Interpretação:

- snapshots JSON repetitivos ganham porque o DNT autentica e descriptografa
  muito menos bytes após a compactação; nesta execução, inflar 1 MiB custou
  menos que AEAD mais digest sobre o ciphertext adicional;
- dados binários determinísticos são praticamente incompressíveis, então o modo
  compactado adiciona CPU e um pequeno overhead de formato;
- segredos pequenos pioram no modo compactado porque o wrapper de compressão
  custa mais bytes e CPU do que economiza;
- arquivos plaintext são mais rápidos e menores quando as propriedades de
  segurança não são necessárias; esse baseline não inclui cifragem,
  autenticação, rotação de chave, vinculação de contexto nem detecção de
  adulteração.

O [relatório medido completo](wiki/benchmarks/dnt-2026-08-02-m1.pt.md) registra
hardware, APFS/SSD, energia AC, Rust/profile, warm-up, amostras, média, desvio,
p95, p99, máximo, throughput, seal/rekey e evidências de memória/CPU não
medidas. Regenere na classe de deployment relevante. DNT é um contêiner de
segurança e portabilidade, não um substituto mais rápido para plaintext
confiável.

## Flags

O campo V1 `flags` é autenticado pelo AAD do header AEAD e é dividido para
evitar combinações impossíveis:

| Faixa | Dono | Regras |
|---|---|---|
| Bits `0..15` | comportamento interno do envelope DNT/AppCore | Somente flags conhecidos por este crate são aceitos. Bits internos desconhecidos falham com `DntError::InvalidFlags` antes de resolver chave ou descriptografar. |
| Bits `16..31` | anotações da aplicação/chamador | DNT autentica e preserva esses bits, mas não atribui semântica central. Chamadores devem alocá-los com `dnt_user_flag(index)`, onde `index` é `0..16`. |

Use `DntFlags`, `dnt_user_flag`, `dnt_compose_flags` ou
`DntSealOptions::with_user_flag` em vez de shifts manuais. Os helpers rejeitam
índices fora da faixa e valores que colocam flags do chamador dentro da faixa
interna.

Modelo de ameaça: DNT protege confidencialidade e integridade contra inspeção
offline e adulteração do arquivo sem a chave. Não protege contra processo
comprometido que possui legitimamente a chave em memória.

**Maturidade:** contrato aditivo pós-RC. Manifest V1 não mudou; deployments
selecionam DNT por configuração existente de providers/capabilities.
