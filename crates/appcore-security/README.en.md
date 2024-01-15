# appcore-security

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Reusable token, secret, authentication and policy contracts.

HashToken values are signed, not encrypted. Manifests contain secret
references, not secret material. Inbound TLS, OAuth, domain authorization and a
production managed vault remain external responsibilities.

`HashTokenProvider::from_secret`, `with_secret` and `with_material` return a
`SecurityResult` and enforce the same minimum secret and salt invariants.
`compute_request_hash` emits a `v2:` SHA-256 value over domain-separated,
length-framed fields with explicit optional-field presence. Earlier
unversioned hashes are rejected, so issuers and validators must upgrade
together.

The 1.0 RC has no TPM or hardware-backed provider. The reviewed 1.1 proposal is
documented in the [public security model](https://wiki.appcore.dnettoraw.com/security/security-model); it introduces no silent
hardware-to-software fallback into the current contracts.

```bash
cargo test -p appcore-security
```
