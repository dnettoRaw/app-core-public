# appcore-bin has been retired

`appcore-bin` no longer provides an AppCore host, CLI, or application facade.
Applications must depend on [`appcore-sdk`](https://crates.io/crates/appcore-sdk)
and select only the capabilities they need.

Read the [SDK documentation](https://docs.rs/appcore-sdk) and the
[migration guide](https://wiki.appcore.dnettoraw.com/crates/appcore-sdk).

This retirement package deliberately contains no executable, compatibility
layer, or Runtime dependency.
