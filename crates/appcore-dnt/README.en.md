# appcore-dnt

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Generic DNT encrypted container contracts and helpers.

Internal dependencies: `appcore-contracts`, `appcore-types`.

Main API: `seal`, `open`, `open_owned`, `inspect_header`, `verify`,
`write_atomic`, `read_verified`, `rekey`, `migrate_envelope`, `DntKeyProvider`,
`DntCodec`, `DntHeader`, `DntContext`, `DntCompression`, `KeyId`, `ContentType`,
`CodecId`, `DntFlags`, `dnt_user_flag`, `dnt_compose_flags` and
`DNT_FLAG_PAYLOAD_DEFLATE`.

DNT is a versioned binary envelope for arbitrary bytes. It authenticates the
canonical header as AEAD additional data and keeps cryptographic keys outside
the file through an explicit key-provider contract. File extensions such as
`.dnt`, `.dntj`, `.dntb` and `.dnto` are usage conventions only.

V1 layout:

```text
canonical header
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
  optional tenant_id
  content_type
  codec_id
  key_id
  public_metadata
ciphertext
  encrypted_metadata_length
  encrypted_metadata
  stored encoded payload
authentication tag
```

The complete header is AEAD additional authenticated data. V1 uses
XChaCha20-Poly1305 with a 256-bit key and a random 192-bit OS nonce.
`DntKeyProvider` resolves keys; they are never stored inside the envelope.

## Why Use DNT

Use DNT for confidentiality, authenticated context, corruption/tamper detection,
atomic writes and verified reads, explicit key rotation with `rekey`, and
explicit envelope migration with `migrate_envelope`. Wrong application, tenant
or logical content type is rejected before plaintext is returned. Storage,
sync and gateways can transport the opaque envelope without understanding the
application domain. Do not use DNT solely to save disk space.

## Compact Mode

Writers can opt into compact payload storage with
`DntSealOptions::compact_payload()` or the authenticated
`DNT_FLAG_PAYLOAD_DEFLATE` flag. Compact mode compresses the codec output with
zlib-wrapped DEFLATE at a balanced level before encryption; normal DNT remains the default.
Compact mode is best for JSON, snapshots, logs and backups. Prefer normal mode
for small, already-compressed, already-encrypted or size-sensitive secret
material. Opening compact envelopes requires `DntOpenOptions.max_payload_bytes`
to bound expansion; V1 readers can inspect either mode.

| Mode | Disk size | Read path |
|---|---|---|
| Normal | Header, encrypted metadata, encoded payload and AEAD tag; size follows codec output. | Read, authenticate, decrypt and decode. Avoids compression work for small or incompressible payloads. |
| Compact | Header, encrypted metadata, compressed codec output and AEAD tag; repetitive data often shrinks, while random or compressed data may grow. | Read, authenticate, decrypt, inflate DEFLATE and decode. Less ciphertext can offset inflation cost for highly compressible data. |

Compression is not a security boundary: file length still reveals approximate
compressed size. Avoid mixing attacker-controlled bytes and confidential
bytes under compression when observable size matters.

The 32-bit header flag field is partitioned. Low bits are reserved for DNT
envelope behavior; high bits are authenticated caller/application flags. Use
`dnt_user_flag`, `dnt_compose_flags`, `DntFlags` or
`DntSealOptions::with_user_flag` instead of manual shifts.

Use DNT when the file must remain portable while still being bound to one
application, tenant, content type, codec and key identifier. It is useful for
snapshots, backup bundles, durable outbox files, sync packages and local secret
material. Plain JSON or raw binary is smaller and faster only when the caller
does not need confidentiality, authenticated metadata, context binding, rekey,
versioned migration or atomic verified writes.

For file reads, prefer `open_owned` or `read_verified` after `fs::read`.
They decrypt the owned envelope buffer in place. Use `open` when the caller only
has a borrowed slice.

`read_verified` requires `DntOpenOptions.max_payload_bytes` and rejects an
oversized file before reading it into a complete buffer. V1 encrypted metadata
is limited to 64 KiB. Call `OpenedDnt::zeroize_plaintext` as soon as returned
plaintext and encrypted metadata are no longer needed.

### Reference Comparison

The release comparator warms each path, separates disk space from latency, and
reports median, p95, p99, maximum, mean, deviation and semantic throughput for
plain reads, DNT open, seal and rekey:

```bash
cargo run -p appcore-dnt --example compare --release
```

Observed on an Apple M1 release run:

- repetitive JSON used 1,048,557 bytes as plaintext, 1,048,746 bytes as normal
  DNT and 4,403 bytes as compact DNT; median warm read/open was 42.7 us,
  5.51 ms and 321.2 us respectively;
- incompressible binary used 1,048,576, 1,048,773 and 1,048,949 bytes; median
  read/open was 42.3 us, 5.51 ms and 6.33 ms;
- a 65-byte secret used 65, 252 and 254 bytes; median read/open was 14.5 us,
  17.7 us and 23.8 us.

The compact JSON path is faster because it authenticates and decrypts about
4 KiB before inflating, instead of processing about 1 MiB of ciphertext. This
does not generalize to incompressible or tiny payloads. The complete environment,
p95/p99, seal, rekey, throughput and limitations are in the
[measured benchmark](wiki/benchmarks/dnt-2026-08-02-m1.en.md). Plaintext is a
performance baseline only and has none of DNT's security properties.
The report also records APFS/SSD, AC power, toolchain/profile, warm-up, samples
and which CPU/memory metrics were not measured. Rerun on the intended deployment
class; these observations are not universal performance guarantees.

## Flags

| Range | Owner | Rules |
|---|---|---|
| Bits 0 through 15 | DNT/AppCore envelope behavior | Only known internal flags are accepted. Unknown bits fail with `DntError::InvalidFlags` before key resolution or decryption. |
| Bits 16 through 31 | Caller/application annotations | Authenticated and preserved without central semantics. Allocate with `dnt_user_flag(index)` for indices 0 through 15. |

Helpers reject out-of-range indices and caller values placed in the internal
range. Prefer them to manual shifts.

Threat model: DNT protects confidentiality and integrity against offline
inspection and tampering without the key. It does not protect against a
compromised process that legitimately holds the key in memory.

Maturity: additive post-RC contract. Manifest V1 is unchanged; deployments
select DNT through existing provider/capability configuration.

```bash
cargo test -p appcore-dnt
```

## Stable documentation

Stable ID: **ACR-007**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-007). This permanent ID
remains valid if the wiki page moves.
