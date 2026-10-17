# Complete Unicode text in PDF (1.0.0-rc.2 correction)

Release candidate `1.0.0-rc.2` includes the variable-content, intrinsic-height, adaptive-pagination, diagnostics, paint-order and PDF Unicode changes below. Earlier source-only status notes describe development stages. Select the exact registry package after verification; external consumer migration remains separate.

Editable and Hybrid PDF export now map each single-glyph shaping cluster to its
complete logical UTF-8 sequence in `ToUnicode`, rather than its first scalar.
The existing PDF writer emits the UTF-16 CMap destinations, including surrogate
pairs. Cluster ends follow sorted logical byte offsets, independent of visual
RTL glyph order. Invalid UTF-8 boundaries, terminal and out-of-range offsets
return `ExportUnsupported` instead of silently omitting text mappings.

A glyph can occur as both `fi` and the literal `ﬁ`, or represent composed and
decomposed text. One font-level mapping cannot describe every occurrence.
Runs with conflicting mappings or multi-glyph clusters retain their original
logical text in PDF `/ActualText` spans. Multi-glyph clusters are not mapped to
the entire source once per glyph. No Unicode normalization is applied.
Readers extracting those context-dependent cases must honor `/ActualText`.

This uses the existing PDF 1.7 output, fonts, glyph IDs, advances and geometry.
There are no new dependencies, public fields, YAML options or APIs. Standard 14
WinAnsi text retains its existing route. Flattened remains vector outlines
without searchable text. This correction changes exporter text mapping, not
layout, shaping, clipping policy, or bidi positioning. RTL extraction tools can
return visual order with direction controls; the sample verifies its complete
unmarked character sequence in either logical or visual order.

Unicode strings retained across font CMaps share an explicit
`ResourceLimits.max_output_bytes` budget, checked before allocation. Temporary
cluster indexing is linear in the existing bounded glyph count; sorting and
lookup add work but do not repeat shaping. Existing scene, text and output
limits remain active. No constant-RSS or export-speed improvement is claimed.

Run `cargo run -p appcore-filemaker --example pdf_unicode --locked` to create
Editable, Hybrid and Flattened samples. They cover `fi/fl/ffi/ffl`, normal words,
literal presentation ligatures, composed/decomposed accents, an isolated
multi-glyph cluster with its own font resource, and Arabic clusters.

After generating them, run
`python3 crates/appcore-filemaker/wiki/audit/verify-pdf-unicode.py`.
Requires Poppler, renders each mode, checks exact Latin scalars without NFC/NFKC
normalization, verifies the fixed RTL sample and outline-only Flattened output,
and compares Hybrid/Flattened pixels. Strict layout audit and preflight remain
separate from text extraction and rendered-page review.

The mixed corpus verifier now checks all resolved element and table-cell text,
not only row IDs and totals. Existing PDFs must be re-exported to receive this
correction. Source-only changes are not available to registry-pinned consumers
until a new exact RC is published and qualified. Included in `1.0.0-rc.2`; external consumer migration remains separate. Rollback selects the
previous qualified exporter release explicitly.
