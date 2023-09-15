# appcore-dnt

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Generic DNT encrypted container contracts and helpers.

DNT is a versioned binary envelope for arbitrary bytes. It authenticates the
canonical header as AEAD additional data and keeps cryptographic keys outside
the file through an explicit key-provider contract. File extensions such as
`.dnt`, `.dntj`, `.dntb` and `.dnto` are usage conventions only.

Writers can opt into compact payload storage with
`DntSealOptions::compact_payload()` or the authenticated
`DNT_FLAG_PAYLOAD_DEFLATE` flag. Compact mode compresses the codec output with
zlib-wrapped DEFLATE at a balanced level before encryption; normal DNT remains the default.
Compact mode is best for JSON, snapshots, logs and backups. Prefer normal mode
for small, already-compressed, already-encrypted or size-sensitive secret
material.

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

```bash
cargo test -p appcore-dnt
```
