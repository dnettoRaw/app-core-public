// =============================================================================
//        #######
//     ###       ###     F: pdf_unicode.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Logical UTF-8 clusters for multi-codepoint `CMaps` and ambiguous-run `ActualText`.
use crate::{ErrorCode, FileMakerError, GlyphRun, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};

struct Cluster<'a> {
    text: &'a str,
    glyphs: usize,
}

fn clusters(run: &GlyphRun) -> Result<BTreeMap<u32, Cluster<'_>>> {
    let mut counts = BTreeMap::<u32, usize>::new();
    for glyph in &run.glyphs {
        *counts.entry(glyph.cluster).or_default() += 1;
    }
    // Shaping order may be RTL; cluster ends come from logical byte order.
    let starts = counts
        .keys()
        .map(|offset| usize::try_from(*offset).map_err(|_| cluster_error()))
        .chain(std::iter::once(Ok(run.text.len())))
        .collect::<Result<Vec<_>>>()?;
    counts
        .into_iter()
        .enumerate()
        .map(|(index, (offset, glyphs))| {
            let text = run
                .text
                .get(starts[index]..starts[index + 1])
                .filter(|text| !text.is_empty())
                .ok_or_else(cluster_error)?;
            Ok((offset, Cluster { text, glyphs }))
        })
        .collect()
}

pub(super) fn collect_run(
    run: &GlyphRun,
    glyphs: &mut BTreeSet<u16>,
    unicode: &mut BTreeMap<u16, String>,
    retained_bytes: &mut usize,
    limits: &ResourceLimits,
) -> Result<()> {
    let clusters = clusters(run)?;
    for glyph in &run.glyphs {
        glyphs.insert(glyph.id);
        let cluster = clusters.get(&glyph.cluster).ok_or_else(cluster_error)?;
        // A multi-glyph cluster has no general glyph-to-text decomposition.
        // Preserve it with ActualText rather than duplicating its source per glyph.
        if cluster.glyphs != 1 || unicode.contains_key(&glyph.id) {
            continue;
        }
        let next = retained_bytes
            .checked_add(cluster.text.len())
            .ok_or_else(|| {
                FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "PDF Unicode mapping size overflow",
                )
            })?;
        ResourceLimits::check("PDF Unicode mapping bytes", next, limits.max_output_bytes)?;
        unicode.insert(glyph.id, cluster.text.to_owned());
        *retained_bytes = next;
    }
    Ok(())
}

pub(super) fn needs_actual_text(run: &GlyphRun, unicode: &BTreeMap<u16, String>) -> Result<bool> {
    let clusters = clusters(run)?;
    if clusters
        .first_key_value()
        .is_some_and(|(offset, _)| *offset != 0)
    {
        return Ok(true);
    }
    for glyph in &run.glyphs {
        let cluster = clusters.get(&glyph.cluster).ok_or_else(cluster_error)?;
        if cluster.glyphs != 1 || unicode.get(&glyph.id).map(String::as_str) != Some(cluster.text) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn cluster_error() -> FileMakerError {
    FileMakerError::new(
        ErrorCode::ExportUnsupported,
        "PDF glyph cluster is not a nonempty logical UTF-8 range",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Glyph, Unit};
    fn run(text: &str, glyphs: &[(u16, u32)], rtl: bool) -> GlyphRun {
        GlyphRun {
            font: "Test".into(),
            text: text.into(),
            rtl,
            width: Unit::ZERO,
            glyphs: glyphs
                .iter()
                .map(|(id, cluster)| Glyph {
                    id: *id,
                    cluster: *cluster,
                    advance_x: Unit::ZERO,
                    advance_y: Unit::ZERO,
                    offset_x: Unit::ZERO,
                    offset_y: Unit::ZERO,
                })
                .collect(),
        }
    }
    fn mappings(run: &GlyphRun) -> BTreeMap<u16, String> {
        let mut unicode = BTreeMap::new();
        collect_run(
            run,
            &mut BTreeSet::new(),
            &mut unicode,
            &mut 0,
            &ResourceLimits::default(),
        )
        .unwrap();
        unicode
    }
    #[test]
    fn rtl_cluster_ends_follow_logical_utf8_byte_order() {
        let run = run("éfi", &[(20, 2), (10, 0)], true);
        let unicode = mappings(&run);
        assert_eq!(unicode[&20], "fi");
        assert_eq!(unicode[&10], "é");
        assert!(!needs_actual_text(&run, &unicode).unwrap());
    }
    #[test]
    fn one_glyph_with_distinct_source_sequences_requires_actual_text() {
        let run = run("fi ﬁ", &[(3, 0), (4, 2), (3, 3)], false);
        let unicode = mappings(&run);
        assert_eq!(unicode[&3], "fi");
        assert!(needs_actual_text(&run, &unicode).unwrap());
    }
    #[test]
    fn multi_glyph_cluster_is_not_duplicated_in_cmap() {
        let run = run("x́", &[(1, 0), (2, 0)], false);
        let unicode = mappings(&run);
        assert!(unicode.is_empty());
        assert!(needs_actual_text(&run, &unicode).unwrap());
    }
    #[test]
    fn invalid_utf8_and_terminal_clusters_fail_explicitly() {
        for offset in [1, 2, 3, u32::MAX] {
            let run = run("é", &[(1, offset)], false);
            assert_eq!(
                clusters(&run).err().unwrap().code(),
                ErrorCode::ExportUnsupported
            );
        }
    }
    #[test]
    fn unicode_mapping_retention_respects_output_budget() {
        let limits = ResourceLimits {
            max_output_bytes: 2,
            ..ResourceLimits::default()
        };
        let mut unicode = BTreeMap::new();
        let error = collect_run(
            &run("ffi", &[(1, 0)], false),
            &mut BTreeSet::new(),
            &mut unicode,
            &mut 0,
            &limits,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::LimitExceeded);
        assert!(unicode.is_empty());
    }
    #[test]
    fn supplementary_unicode_uses_a_complete_utf16_surrogate_pair() {
        use pdf_writer::types::{SystemInfo, UnicodeCmap};
        use pdf_writer::{Name, Str};
        let unicode = mappings(&run("𝄞", &[(1, 0)], false));
        let mut cmap = UnicodeCmap::<u16>::new(
            Name(b"Test"),
            SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Identity"),
                supplement: 0,
            },
        );
        cmap.pair_with_multiple(1, unicode[&1].chars());
        assert!(String::from_utf8(cmap.finish().into_vec())
            .unwrap()
            .contains("<0001> <D834DD1E>"));
    }
}
