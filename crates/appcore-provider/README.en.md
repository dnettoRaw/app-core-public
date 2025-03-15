# appcore-provider

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Implementation-neutral provider roles, plans, factories, registry, secret
resolution and coordination/job contracts.

Deployment providers are explicit. Unavailable providers fail bootstrap; the
registry never silently substitutes another implementation.

Filesystem leases persist a versioned epoch high-water sidecar independently
from the active lease. Releasing a lease never resets its fencing sequence.
Writers remain responsible for checking the current token immediately before
every protected write.

```bash
cargo test -p appcore-provider
```
