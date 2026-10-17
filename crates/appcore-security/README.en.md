# appcore-security

Release candidate **`appcore-security 1.0.3-rc.5`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.


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

## Windows private directories

This section describes unpublished source changes. Verify the exact registry package before consumer adoption.

`create_update_private_directory` accepts a private leaf beneath handle-validated trusted Windows ancestors (current user, SYSTEM, Administrators or TrustedInstaller). Other replacement grants and reparse points are rejected. Keep the returned guard alive during work: it requests `FILE_LIST_DIRECTORY` and excludes delete sharing. Existing insecure leaves are rejected without repair. New private paths explicitly set both owner SID and protected DACL; Windows keyring directory flush opens with write access. Run the native directory and DPAPI tests under both limited and elevated interactive tokens before replacing local overrides.

## Release candidate — 2026-10-14

`1.0.3-rc.3`: source changes.

## Opt-in private test fixtures (unpublished source)

Enable `test-fixtures` only in explicit test consumers. `test_fixtures::random_fixture_secret` returns 32 OS-random bytes in a redacted zeroizing `SecretBytes`. `write_secret_fixture(&guard, name, &secret)` creates one new flat file (name at most 128 ASCII alphanumeric/`-_.` bytes, content 1..=65536 bytes), revalidates the private root and refuses existing files, symlinks, traversal or insecure directories. Unix creates 0600 before writing; Windows assigns the actual current-token owner and protected owner-only DACL with the existing production native ACL code before any secret bytes are written. The production secret-file validator checks the created handle. The helper never repairs existing paths, invokes PowerShell or logs secret/error contents.

Keep the `PrivateDirectoryGuard` alive, generate unpredictable credentials and remove the fixture directory after dropping guards. Writes sync the file; this is test support, not an encrypted store, production credential provisioner or a power-loss persistence contract. A failed write attempts to remove only its own newly created leaf. Existing data is never overwritten. Native limited/elevated Windows execution is still required to qualify Windows ACL behavior; cross-compilation alone is not that evidence. No secret format, token contract or default feature behavior changes. These APIs require a future immutable published package before registry-only application adoption.
