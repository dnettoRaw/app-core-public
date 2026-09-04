# Deterministic multilingual test fonts

These test-only subsets are licensed under the SIL Open Font License 1.1 in
`../../examples/assets/OFL-1.1.txt`.

- `NotoSansArabic-Test.ttf` is a FontTools subset of
  `google/fonts/ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf` at the
  upstream `main` revision retrieved on 2026-08-30. Its SHA-256 is
  `c3e75d3b304810f42a48e3fab0bc685e789e941ebeeae9e7636ca845c780c2d3`.
- `NotoSansJP-Test.ttf` is a FontTools subset of
  `google/fonts/ofl/notosansjp/NotoSansJP[wght].ttf` at the
  upstream `main` revision retrieved on 2026-08-30. Its SHA-256 is
  `655ab0eac9f1154c956d77ad2f8d77a78ed967dcf723aa27dd51aa17544d8d73`.

The subsets retain all layout features and only the glyphs used by the
multilingual end-to-end fixtures. They are evidence for explicit fallback,
Arabic shaping, Japanese measurement, and exporter font embedding; they are
not general-purpose application fonts.
