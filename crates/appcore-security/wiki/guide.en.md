# appcore-security

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** reusable Runtime authentication, token, secret and policy
contracts.

**Internal dependencies:** `appcore-core`, `appcore-dnt`.

**Primary API:** HashToken provider, claims, command token factory/validator,
request hashing, `SecurityError`; secret references, resolvers, stores,
zeroizing bytes, file keyring, secret metadata/rotation format, Vault contract,
peer credentials, DNT key-provider adapter, authentication and policy traits.

Use it for infrastructure authentication and secret indirection. Tokens are
signed, not encrypted. Do not place domain authorization, OAuth servers,
inbound TLS or a managed vault implementation here.

`HashTokenProvider::from_secret`, `with_secret` and `with_material` return a
`SecurityResult` and enforce the same minimum secret and salt invariants.
`compute_request_hash` emits a `v2:` SHA-256 value over domain-separated,
length-framed fields with explicit optional-field presence. Earlier
unversioned hashes are rejected, so issuers and validators must upgrade
together.

## Windows DPAPI provider in `1.0.2-rc`

`WindowsDpapiSecretKeyring` protects each bounded key record with non-interactive
current-user/current-machine DPAPI. The keyring also requires a protected
owner-only DACL, rejects symlinks, junctions and other reparse points, and
zeroizes plaintext owners. Select `windows-dpapi-user-v1` explicitly; an
existing `file-keyring-v1` directory is rejected by the format marker instead
of being converted or used as fallback.

The same user on the same machine may restore a complete provider-directory
backup after it decrypts and validates. Another user or machine must fail
closed. Real multi-user/multi-machine certification remains pending under
AC-009, so the RC is implementation preview evidence, not a production
certification. Stable 1.0 behavior is unchanged and upgrading is explicit.

**Maturity:** stable RC contracts; production suitability depends on selected
secret backend and deployment controls.
