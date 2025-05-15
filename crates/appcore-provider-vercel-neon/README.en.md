# appcore-provider-vercel-neon

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Official isolated factory for the Vercel API control-plane adapter backed by an
externally operated Neon coordination service.

Runtime nodes receive only the Vercel endpoint and an auth-token reference.
Neon credentials and schema operations remain outside the nodes.

```bash
cargo test -p appcore-provider-vercel-neon
```
