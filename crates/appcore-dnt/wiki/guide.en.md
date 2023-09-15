# appcore-dnt

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** generic DNT encrypted container contracts and helpers.

**Internal dependencies:** `appcore-contracts`, `appcore-types`.

**Primary API:** `seal`, `open`, `open_owned`, `inspect_header`, `verify`,
`write_atomic`, `read_verified`, `rekey`, `migrate_envelope`,
`DntKeyProvider`, `DntCodec`, `DntHeader`, `DntContext`, `DntCompression`,
`KeyId`, `ContentType`,
`CodecId`, `DntFlags`, `dnt_user_flag`, `dnt_compose_flags` and
`DNT_FLAG_PAYLOAD_DEFLATE`.

DNT is a binary envelope for arbitrary bytes. `.dnt`, `.dntj`, `.dntb` and
`.dnto` are conventions only; consumers inspect the authenticated header.

The V1 layout is:

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

The whole header is AEAD additional authenticated data. V1 uses
XChaCha20-Poly1305 with a 256-bit key and a 192-bit OS-random nonce. Keys are
resolved by `DntKeyProvider`; they are never embedded in the envelope.

## Why Use DNT

DNT is not meant to replace every file. It is useful when bytes need to move
between storage providers, backup flows, sync transports or local secret stores
without losing security properties.

Use DNT when the file needs:

- confidentiality without putting the key beside the encrypted bytes;
- authenticated identity for application, tenant, content type, codec and key;
- rejection of wrong application, wrong tenant or wrong logical content before
  plaintext is returned;
- corruption and tamper detection across both header and payload;
- atomic write and verified read helpers;
- explicit key rotation through `rekey`;
- explicit envelope migration through `migrate_envelope`;
- opaque transport through storage, sync or gateway components that must not
  understand the domain payload.

Do not use DNT merely to save disk. Plain JSON or raw binary is simpler, smaller
and faster when there is no need for encryption, authentication, context
binding, rekey or versioned migration.

## Compact Mode

Normal DNT stores the codec output directly before encryption. Compact DNT sets
the authenticated `DNT_FLAG_PAYLOAD_DEFLATE` flag and stores a zlib-wrapped
DEFLATE stream at a balanced compression level before encryption. Existing V1
readers can inspect both modes; opening compact envelopes requires
`DntOpenOptions.max_payload_bytes` so decompression is bounded.

For complete file buffers, prefer `open_owned` or `read_verified`; they decrypt
the owned envelope in place. Use `open` when the caller only has a borrowed
slice.

`read_verified` requires an explicit `DntOpenOptions.max_payload_bytes` and
rejects an oversized file before complete-buffer allocation. V1 encrypted
metadata is limited to 64 KiB. `OpenedDnt::zeroize_plaintext` clears returned
plaintext and encrypted metadata when the caller no longer needs them.

| Mode | Disk size | Read path |
|---|---|---|
| Normal | Header + encrypted metadata + encoded payload + AEAD tag. Size tracks the codec output and has the lowest CPU cost. | Read, authenticate, decrypt, then codec decode. This is the fastest CPU path for small or incompressible files. |
| Compact | Header + encrypted metadata + compressed encoded payload + AEAD tag. Repetitive JSON, snapshots and logs are commonly much smaller; already-compressed or random payloads can be the same size or larger. | Read fewer bytes from disk, then authenticate, decrypt, DEFLATE-inflate and codec decode. Inflate adds work, but less ciphertext can reduce AEAD and digest work enough to improve total latency for highly compressible payloads. |

Compact mode must not be treated as a security boundary. File size still leaks
an approximate compressed length. Avoid compacting secrets that mix
attacker-controlled bytes with confidential bytes when size observation matters.

### Reference Comparison

This repository includes a reproducible comparison that writes each sample as a
plain file, normal DNT and compact DNT. It warms every path and reports separate
disk-space, read/open, seal and rekey distributions:

```bash
cargo run -p appcore-dnt --example compare --release
```

Reference Apple M1 release run, shown as separate comparable categories:

Disk space:

- repetitive JSON: plaintext 1,048,557 bytes; normal 1,048,746; compact 4,403;
- incompressible binary: plaintext 1,048,576 bytes; normal 1,048,773; compact
  1,048,949;
- small secret: plaintext 65 bytes; normal 252; compact 254.

Median warm-cache read path:

- repetitive JSON: plaintext 42.7 us; normal read/open 5.51 ms; compact
  read/open 321.2 us;
- incompressible binary: plaintext 42.3 us; normal 5.51 ms; compact 6.33 ms;
- small secret: plaintext 14.5 us; normal 17.7 us; compact 23.8 us.

Interpretation:

- repetitive JSON snapshots benefit because DNT authenticates and decrypts far
  fewer payload bytes after compression; inflating 1 MiB was cheaper than AEAD
  plus digest over the additional ciphertext in this run;
- deterministic binary data is effectively incompressible, so compact mode adds
  CPU and a tiny format overhead;
- tiny secrets are worse in compact mode because the compression wrapper costs
  more than it saves;
- plain files are faster and smaller when security properties are not needed;
  their baseline does not include encryption, authentication, key rotation,
  context binding or tamper detection.

The [complete measured report](benchmarks/dnt-2026-08-02-m1.en.md) records
hardware, APFS/SSD, AC power, Rust/profile, warm-up, sample counts, mean,
deviation, p95, p99, maximum, throughput, seal/rekey results and unmeasured
memory/CPU evidence. Regenerate it on the deployment class that matters. DNT is
a security and portability container, not a faster replacement for trusted
plaintext files.

## Flags

The V1 `flags` field is authenticated by the AEAD header AAD and is partitioned
to avoid impossible combinations:

| Range | Owner | Rules |
|---|---|---|
| Bits `0..15` | DNT/AppCore internal envelope behavior | Only flags known by this crate are accepted. Unknown internal bits fail with `DntError::InvalidFlags` before key resolution or decrypt. |
| Bits `16..31` | Caller/application annotations | DNT authenticates and preserves them but does not attach core semantics. Callers should allocate them with `dnt_user_flag(index)` where `index` is `0..16`. |

Use `DntFlags`, `dnt_user_flag`, `dnt_compose_flags` or
`DntSealOptions::with_user_flag` instead of raw bit shifts. The helpers reject
out-of-range user indexes and reject values that place caller flags inside the
internal range.

Threat model: DNT protects confidentiality and integrity of stored bytes
against offline file inspection or tampering without the key. It does not
protect against a process that is legitimately holding the key in memory.

**Maturity:** additive post-RC contract. Manifest V1 is unchanged; deployments
select DNT through existing provider/capability configuration.
