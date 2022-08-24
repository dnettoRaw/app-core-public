# AppCore published crates

This repository is a source mirror of AppCore crates that are already
available on [crates.io](https://crates.io/search?q=appcore-). It does not
contain unpublished development work.

The `main` branch contains the latest stable AppCore release. The `beta`
branch adds published prerelease crates without changing the stable snapshot.
Every directory under `crates/` is extracted from the corresponding `.crate`
archive. Archive versions and SHA-256 checksums are recorded in
[`RELEASES.md`](RELEASES.md).

The repository's early milestone commits are an explicitly documented
reconstruction. See [`HISTORY.md`](HISTORY.md) for the provenance and limits of
those dates.

Public architecture, tutorials, and crate documentation are available in the
[AppCore Runtime wiki](https://wiki.appcore.dnettoraw.com/).

## Using a crate

Add the required package from crates.io:

```toml
[dependencies]
appcore-bin = "1.0.0"
```

Individual source snapshots can also be checked directly:

```bash
cargo test --manifest-path crates/appcore-bin/Cargo.toml
```

## License

AppCore is available under the [MIT License](LICENSE).

