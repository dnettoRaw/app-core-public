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

The 1.0 RC has no TPM or hardware-backed provider. ADR 0005 records an additive
1.1 proposal with explicit fallback and physical-hardware evidence; the current
Runtime makes no hardware-security claim.

**Maturity:** stable RC contracts; production suitability depends on selected
secret backend and deployment controls.
