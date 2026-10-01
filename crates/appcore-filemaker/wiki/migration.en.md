# Migrating declarative layouts

This guide describes the additive FileMaker YAML and Rust API changes in the
current beta. Pin the crate version and font files used to produce every
reference PDF; exporter `losses=0` is not visual-parity evidence.

## Text alignment and padding

- Replace space/NBSP-based numeric-column alignment with `text_options.align_x` or
  `table.columns[].align_x` (`start`, `center`, `end`). Alignment uses final
  shaped line widths after wrapping and shrinking.
- Use `text_options.padding_inline` for symmetric text insets that survive
  wrapping. Set `table.columns[].padding: { top, right, bottom, left }` for
  per-side cell insets; values must be nonnegative absolute or logical lengths.
  Use `text_options.padding: { top, right, bottom, left }` for per-side text
  block insets; these reduce measured content bounds and are shared by
  exporters and expanded-text pagination. Nonnegative absolute/logical values
  and percentages below 50% are supported; `auto` is rejected.
- Rust struct literals initialize the new fields listed in the beta3 migration
  section below. Use each type's `Default` where available to preserve default
  behavior. Older serialized text lines default `source_text` to empty; reshape
  text before using line pagination.

## Rust struct-literal changes from beta3

The YAML `filemaker: "1.0"` contract receives compatible optional fields, but
the beta Rust API is not source-compatible for exhaustive public struct literals.
The beta3-to-beta5 additions detected by `cargo-semver-checks` are:

- `TextOptions`: `align_x`, `padding_inline`; `TextSourceOptions`:
  `align_x`, `padding_inline`, `padding`; `TextIr`: the same three; `TextLayout`:
  `paint_offset_y`, `align_x`, `padding_inline`, `padding`; `TextLine`:
  `source_text`.
- `ElementSource` and `ElementIr`: `keep_with_next`, `text_segments`.
- `StyleSource`, `Style`, and `ComputedStyle`: `stroke_sides`, `line_height`,
  `underline`.
- `TableColumn` and `ResolvedTableColumn`: `align_x`, `padding`;
  `ResolvedTableCell`: `padding`.
- `TableSource`: `keep_together_by`, `row_anchor_field`, `page_bodies`;
  `TableSpec`: `keep_together_by`, `row_anchor_field`; `TableIr`: `page_bodies`;
  `TablePage`: `row_padding`.
- `TableStyleRuleSource` and `TableStyleRule`: `padding`,
  `padding_first_page`, `padding_continuation`, `text_offset_y`,
  `text_offset_y_first_page`, `text_offset_y_continuation`, `min_height`,
  `min_height_first_page`, `min_height_continuation`, `reserve_after`.

External Rust consumers constructing these public values directly must add the
listed fields (or use `..Default::default()` only for types that implement
`Default`). This is a beta migration, not a claim that beta3 Rust literals
compile unchanged. The semver checker classifies beta3-to-beta5 as a major
prerelease step; forcing patch compatibility fails on the added fields.

## Pagination

- Keep related sibling elements together with `keep_with_next` through the
  penultimate visible sibling in a vertical flow.
- For table rows, supply a non-null metadata key on every row and set
  `table.keep_together_by: layout_group`. Contiguous equal keys stay on one
  page if their measured height fits. `group_by` only marks starts and is not
  a keep guarantee. Oversized groups split between complete rows; each row
  must fit a page. Represent a logical block taller than a page as multiple
  component rows under the same key; put values included in totals on only one
  component row to prevent duplicate aggregation.
- Horizontal text with `overflow: expand` in a vertical flow splits at shaped
  line boundaries. A line taller than the page content area fails explicitly.
- Named anchors on paginated tables resolve to the last physical fragment. To
  target a particular row, declare `row_anchor_field` and put a unique bounded
  string in that metadata field; for example `anchors: { top:
  'line-items::totals.bottom+4pt' }` targets the row named `totals` and follows
  it to its physical page. `null` or a missing field publishes no anchor.
- To keep a following element on an anchored row's page, add positive absolute
  `reserve_after` to a matching conditional row rule. See the executable
  `examples/row-anchor-reserve.yml` and its paired data file.
- A matching `conditional_styles` rule can set `min_height: 18pt`. The row uses
  the larger of its measured content height and all matching minimums, providing
  explicit type-specific spacing without oversized-font spacer rows. Use
  `min_height_first_page` and `min_height_continuation` when page regions need
  different spacing; the paginator measures against the destination page.
- Set `style.line_height: 1250000` in a conditional row style to override the
  shared text line-height (millionth ratio, bounded from 500000 to 4000000).

Use per-side `padding` on matching `conditional_styles` rows for visual
indentation. The last matching padding rule wins and is added to column padding;
do not prefix row data with spaces to simulate layout.
Use `padding_first_page` or `padding_continuation` on a conditional rule when
the same row needs different insets by destination page. An unset page-specific
value falls back to that rule's `padding`.

## Remaining boundaries

Rich-text runs and barcode encoding (including EAN13) are not supported.
Standalone text supports per-side block padding through `text_options.padding`;
it is measured and exported consistently. Cell borders can select edges with
`style.stroke_sides`; `style.underline` draws each measured horizontal text line.
Use `table.page_bodies.first` and `.continuation` with `offset_y`
and `height` for different first-page and continuation table regions. Compose
styles as separate table rows using
`conditional_styles` and `keep_together_by` when that row model is sufficient.
The Runtime lays out and exports typed application data; domain calculations
and labels remain in the application. Application-specific document samples
belong in their application repositories, not this generic crate.

Regenerate every page and inspect rasterized output after migration. Include
one-, two-, three-, and four-or-more-page fixtures, annotations near breaks,
long text, varied numeric widths, multiple grouped rows, and final rows near
the page bottom.
