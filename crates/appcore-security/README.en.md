# appcore-security

Bearer V1 limits are exposed in `appcore_security::token`: decoded JSON claims
are limited to 64 KiB, provider signatures to 256 KiB, and the hex envelope to
655,364 bytes. Both components are checked before decoding or crypto; excess
returns `CommandTokenError::InvalidFormat`. Issuance bounds input fields and
JSON escaping before signing, and rejects empty or oversized provider output.
These are security admission limits, not a new wire format; previously oversized
tokens must be reissued with smaller claims. Provider-internal allocations and
caller-owned input buffers are not controlled by this boundary. HTTP ingress
may impose a smaller limit. Tokens must not carry application payloads.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Reusable token, secret, authentication and policy contracts.

`create_private_directory` and `open_private_directory` provide an
owner-controlled filesystem boundary. They reject symlink/reparse components,
validate writable ancestors, pin directory handles during the guarded
operation, and fail closed on unsupported platforms. Existing permissions and
ACLs are never silently repaired.
Use `PrivateDirectoryGuard::join` for child paths; it rejects absolute paths,
`.`/`..` traversal and control characters before a consumer opens the result.

**Responsibility:** reusable authentication, token, secret and policy
contracts.

**Internal dependencies:** `appcore-core`, `appcore-dnt`.

**Main API:** HashToken provider, claims, command-token factory/validator,
request hash and `SecurityError`; secret references, resolvers, stores,
zeroized bytes, file keyring, metadata/rotation, Vault contract, peer
credentials, DNT key-provider adapter, authentication traits and policy.

HashToken values are signed, not encrypted. Manifests contain secret
references, not secret material. Inbound TLS, OAuth, domain authorization and a
production managed vault remain external responsibilities.

`HashTokenProvider::from_secret`, `with_secret` and `with_material` return a
`SecurityResult` and enforce the same minimum secret and salt invariants.
`compute_request_hash` emits a `v2:` SHA-256 value over domain-separated,
length-framed fields with explicit optional-field presence. Earlier
unversioned hashes are rejected, so issuers and validators must upgrade
together.

`RequestValidationDetailsRef` and `RequestPayloadRef` provide an additive
borrowed path for in-flight requests. `compute_borrowed_request_hash` preserves
the exact V2 output while counting and hashing structured JSON directly in two
passes, without retaining a complete encoded payload. The owned contract stays
available for compatibility.

`CommandTokenValidator` centrally rejects issue timestamps in the future,
invalid timestamp ordering and claim lifetimes above `TokenClaims::ttl_ms`.
Callers that coordinate distinct clocks may opt into at most five minutes of
positive issue-time skew with `with_clock_skew_ms`; expiry remains strict.

The `1.0.2-rc` adds the Windows-only `WindowsDpapiSecretKeyring`. Its records are
protected for the current user on the current machine, keep owner-only ACLs and
reject reparse points. Runtime composition selects it explicitly with
`windows-dpapi-user-v1` and `provider:active`; it never falls back to the file
keyring or machine-wide DPAPI. Multi-user and multi-machine Windows
certification remains pending under AC-009, so this prerelease is not yet a
production certification claim.

The original stable 1.0.0 package had no TPM, DPAPI or hardware-backed
provider. Selecting the additive `1.0.2-rc` DPAPI provider is explicit and does
not change the existing file-keyring behavior.

**Maturity:** stable RC contracts; production depends on the selected secret
backend and deployment controls.

```bash
cargo test -p appcore-security
```

## Stable documentation

Stable ID: **ACR-010**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-010). This permanent ID
remains valid if the wiki page moves.
