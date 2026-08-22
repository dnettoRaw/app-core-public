# AI threat model

[Português](threat-model.pt.md) | [Français](threat-model.fr.md) |
[Guide](guide.en.md) | [Generative LLMs](generative-llm.en.md)

Scope: `appcore-ai 0.1.0-beta.1`, optional Candle/OpenAI-compatible backends,
the opt-in `appcore-bin` component and experimental Swarm boundaries. The crate
does not claim process sandboxing or zero trust.

| Threat | Control | Residual limitation |
|---|---|---|
| malicious, replaced or poisoned model | exact size + SHA-256 identity; optional publisher provenance; verified bytes before activation | trust policy must choose publishers |
| path traversal/symlink substitution | digest-derived filenames, canonical root, no-follow open, opened-handle metadata/size validation, exclusive temporary file and atomic create-without-replacement activation | local administrator retains host authority |
| decompression bomb/oversized tensor | default native format is uncompressed; bounded artifact, dimensions, classes, inputs, outputs, RAM/VRAM estimates | optional external formats need backend-specific parsers |
| metadata spoofing/custom ops | registry validation and `ModelSecurityPolicy`; provider formats denied by default | enabling arbitrary provider formats may execute backend code |
| prompt/content or credential leakage | redacted `Debug`, payload-free observations, secret references only | application callbacks can still leak data if written unsafely |
| multi-tenant/cross-tenant routing | explicit authorization context and exact remote grants; bridge validates tenant | host adapter is part of the trusted computing base |
| native backend crash | optional feature, bounded inputs and translated errors | Candle runs in process; there is no crash sandbox |
| hardware probe abuse or control | bounded read-only OS/sysfs/NVML queries, no shell/WMI process and no fan/clock/voltage/power write API | local administrator and kernel/driver remain trusted; diagnostics reveal coarse capacity |
| resource exhaustion/DoS | single-flight out-of-lock probe, governor, admission, deadlines, cancellation and fixed bounds for queues/batches/registries/routes/residents/peers/transfers | `Unrestricted` deliberately reduces voluntary headroom |
| training poisoning | bounded explicit dataset, reproducible seed, verified resume/checkpoint identity and publisher metadata | dataset truth/quality cannot be inferred by the Runtime |
| malicious peer or compromised discovery | AppCore authenticator boundary, strictly newer expiring advertisements, duplicate-claim rejection, tenant grants and contribution caps | advertised load/performance may be dishonest |
| artifact poisoning or withheld chunks | end-to-end digest/size/provenance, transfer deadline, bounded alternate stores | repeated malicious availability can consume bounded retry budget |
| replayed execution | Swarm bridge contract requires AppCore Peer RPC replay protection, expiry and nonce validation | not implemented as a second replay store in this crate |
| fake availability/peer churn | leases, health/cost penalty and bounded failover | work already running may fail and be retried only when policy permits |
| untrusted remote result | authenticated target, bounded response and explicit diagnostics | generic remote inference correctness is not cryptographically provable |

## Additional generative LLM threats

| Threat | Implemented or required deployment control | Residual limitation |
|---|---|---|
| exposed model server | loopback bind, host authentication, deployment firewall | local administrator controls the process |
| prompt injection triggers a tool | application-owned tools and authorization; output never becomes a command automatically | untrusted content still influences the model |
| replaced chat template/tokenizer | exact model binding plus artifact digest/revision; bundle ranges have individual digests | generic HTTP cannot prove bytes loaded by an external server |
| context/KV-cache DoS | token, context, sequence, queue, and memory bounds before dispatch | only the engine knows exact tokenization |
| ignored engine option | capability negotiation and explicit error for unsupported sampling/tools | OpenAI-compatible implementations are not semantically identical |
| partial output after cancellation | current adapter is non-streaming and discards failed exchanges; timeout is bounded | blocking transport observes cancellation only before/after the exchange |
| compromised native engine | isolated process, immutable path, unprivileged user, Supervisor | strong sandboxing belongs to deployment |
| corrupted segmented-model range | bundle tied to complete identity, non-overlapping bounded ranges and SHA-256 per loaded segment | NVMe and local admin remain trusted |

The LLM server receives no direct filesystem tools by default. OpenAI HTTP
compatibility is transport, not a security boundary or proof of sampling
equivalence.

Security invariants:

- local privacy and resource mode always override remote demand;
- a peer cannot force `Unrestricted` or expand contribution policy;
- model bytes never travel inside generic capability command RPC;
- no raw prompt, output, token, secret or private model URL is emitted by
  built-in telemetry;
- signed artifacts fail closed when provenance is absent, invalid or expired;
- unknown capacity is not treated as unlimited.

The corruption fixture is
[`tests/fixtures/corrupt-native-linear-v1.artifact`](../tests/fixtures/corrupt-native-linear-v1.artifact).
Deterministic byte sweeps and three `cargo-fuzz` targets exercise the native
parser, contract boundaries and bounded OpenAI-compatible response decoder.
Tests additionally race 32 writers against one artifact identity and reject
Unix symlink substitution on full, range and existence reads.
