# Mixed variable-content corpus (1.0.0-rc.2)

Release candidate `1.0.0-rc.2` includes the variable-content, intrinsic-height, adaptive-pagination, diagnostics, paint-order and PDF Unicode changes below. Earlier source-only status notes describe development stages. Select the exact registry package after verification; external consumer migration remains separate.

Run `cargo run -p appcore-filemaker --example variable_corpus --locked`.
One YAML design and a separate adaptive plan bind four synthetic typed JSON datasets.
The outputs under `target/filemaker-examples/variable-corpus-*` include PDF,
resolved scene, strict preflight, pagination decisions, debug overlay and SVG masks.
This increment also fixes page-band paint precedence in the existing engine. No dependency, public API, serialized field or published version changes.

| Pages | Groups | Source rows |
|---|---|---|
| 1 | 3 | 9 |
| 2 | 12 | 36 |
| 5 | 40 | 120 |
| 20 | 190 | 570 |

Each group contains an item, a variable-length description and a comment with a
detailed discount. The three rows share `keep_together_by`; `oversized_group: error`
explicitly rejects a description that cannot fit a page. The test also checks
logical shaped text and measured cell heights, not only retained source strings.
The two-column automatic header, dependent title and first body share native
measurement. Three displayed TVA rates, pre-discount values, discount details,
net/tax/final totals, conditions, signature and an EAN-13 are consumer-supplied
fixture data. FileMaker performs no fiscal calculations or business validation.
The fixture totals are internally consistent, but this corpus is not fiscal certification.

The first old value has a line over its cell, using an explicit local overlay.
The watermark on every page and the first-page stamp use existing page bands:
those bands already have a non-colliding paint contract and reject an explicit
`collision` field. Global body collision detection stays enabled. Sparse row
anchors must be omitted or null where absent, not repeated empty strings.
Body X coordinates are relative to the useful-region origin; a regression checks
that the strike stays inside its intended value cell after applying that origin.
This corrected fixture configuration, not an engine or shaping defect.

Tests assert exactly-once row order, complete cell text, intact groups, expected
page counts, local strike geometry and final-only closing descendants, including
barcode and signature. Strict `audit_layout` and export preflight both run.
The PDF pages must also be rendered and visually reviewed: exporter loss counts
alone cannot approve layout. The earlier ligature-extraction finding is corrected in the 1.0.0-rc.2 exporter; full clusters and context-specific `/ActualText` preserve logical text. Continue independent text-extraction and visual checks. SVG masks show geometric occupancy, while
`audit_layout` applies intentional-overlap policies.

## Consumer adoption gate

A consumer pinned to the registry package cannot execute source-only methods.
Do not silently patch it to a local crate or modify its dependency pin before a
new independent RC is published and qualified. Prepare adoption as follows:

1. Select the new exact registry RC explicitly once available.
2. Move variable identification and title into ordinary body roots; keep decorative
   page bands fixed. Use `height: auto`, two-column flow and anchors in YAML.
3. Replace fixed first/continuation reserves with an explicit adaptive plan,
   `first_after` and a final closing root. Remove competing `reserve_after` and
   built-in table totals from that selected design; totals remain bound data.
4. Call `resolve_adaptive` or `diagnose_adaptive`, then enforce layout audit and
   strict export preflight. Remove manual header measurement and YAML coordinate
   substitution only after matching the real consumer’s synthetic regressions.
5. Verify native fonts, short/long headers, conditional closing blocks, discounts,
   barcodes and rendered outputs. Preserve the previous application release,
   template and integration as an explicit rollback artifact.

The external consumer has not been modified. Select the exact registry release `=1.0.0-rc.2` after verification.

Reproduce the PDF checks after running the example: `python3 crates/appcore-filemaker/wiki/audit/verify-variable-corpus.py`. Requires Poppler, verifies ordered item/description/comment IDs, supplied summary lines, 95 rendered EAN-13 modules and quiet zones, and renders all 28 pages. It does not replace visual review or scanner certification.

The watermark belongs in `page.master.background`. `z_index` orders paint within a layer; a negative value in `header` does not move the watermark behind body glyphs. The corpus regression first failed with the header band, then checks the resolved background layer and paint order. Strict preflight had passed the obscuring configuration.

The corpus exposed an engine defect as well: lexical sorting placed the empty body layer before `!page-background`, and custom Unicode layer names could sort after the foreground band. The compatible correction assigns page-band precedence explicitly: background, body, header/footer. Body layer names retain their lexical order, then z-index and source sequence. Tests cover empty/custom/Unicode layers, extreme z-index values and actual raster pixels. No spatial-index replacement or new API is involved.

Recompute previously cached resolved scenes when adopting the paint-order correction; do not reuse a cache produced by an earlier source binary. Rollback selects the previous qualified application/crate release explicitly.
