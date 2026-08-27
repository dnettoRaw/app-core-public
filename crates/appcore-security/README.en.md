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

The 1.5 alpha adds the Windows-only `WindowsDpapiSecretKeyring`. Its records are
protected for the current user on the current machine, keep owner-only ACLs and
reject reparse points. Runtime composition selects it explicitly with
`windows-dpapi-user-v1` and `provider:active`; it never falls back to the file
keyring or machine-wide DPAPI. Multi-user and multi-machine Windows
certification remains pending under AC-009, so this prerelease is not yet a
production certification claim.

The stable 1.0 line has no TPM, DPAPI or hardware-backed provider. Selecting
the 1.5 prerelease is explicit; the new provider does not change stable keyring
behavior.

```bash
cargo test -p appcore-security
```
