# Candle ZIP security distribution

`appcore-ai 1.0.0-rc.2` selects `dnet-candle-core =1.0.0-rc.1` and `dnet-candle-nn =1.0.0-rc.1` from crates.io for the optional Candle backend. They are downstream security distributions of Hugging Face Candle 0.11.0, not official upstream releases or new Runtime modules.

The official 0.11.0 core requires ZIP 8.6.x, affected by RUSTSEC-2026-0367, RUSTSEC-2026-0368 and RUSTSEC-2026-0369. The security core pins `zip =9.0.4`; NN selects that same core identity. Three NPZ/PTH filename call sites propagate ZIP 9's fallible API. Tensor/NN operations are retained. Source archive SHA-256, original Git revision, exact source differences and licenses are preserved in the distribution repository. No advisory ignore, hidden local patch or Git dependency is shipped.

`backend-candle`, `training-candle`, the `NativeLinearV1` format, backend IDs and public Rust fields are unchanged. The default build still has no ML framework dependency. Applications use the existing AppCore facade and data contracts; no Candle tensor type is added to the public facade. CPU validation is not CUDA/Metal/MKL certification.

Regressions cover Unicode NPZ roundtrip, CP437 filename decoding and controlled errors for malformed archives through eager/lazy NPZ and PTH readers. Existing upstream CPU tests and all-feature AppCore AI tests run separately. ZIP 9 retains CP437 decoding; invalid UTF-8 bytes are not automatically rejected as filename errors.

Adoption explicitly pins `appcore-ai = "=1.0.0-rc.2"` after registry qualification and refreshes the lockfile, including optional features. Confirm with `cargo tree --all-features -i zip` and `cargo audit`. Existing exact pins do not upgrade themselves. Restore a complete previous graph only after assessing its known ZIP 8 vulnerabilities. Prefer a qualified official Candle release when its registry graph supports ZIP >=9; do not silently mix core package identities.

[Source and provenance](https://github.com/dnettoRaw/candle-zip9-security)
