// =============================================================================
//        #######
//     ###       ###     F: adaptive_pagination.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

use appcore_filemaker::*;
use std::collections::BTreeMap;

fn fonts() -> FontManager {
    let mut fonts = FontManager::default();
    fonts
        .register(
            FontAsset::new(
                "Body",
                include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    fonts
}
fn document(rows: usize) -> DocumentIr {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(include_bytes!("../wiki/audit/adaptive-document.yml"))
        .unwrap();
    let rows = (0..rows)
        .map(|i| {
            DataValue::Object(BTreeMap::from([
                (
                    "content".into(),
                    DataValue::String(format!("Entry {i:03}\nDescription {i:03}\nComment {i:03}")),
                ),
                ("group".into(), DataValue::Integer(i as i64)),
            ]))
        })
        .collect();
    compiler
        .bind(
            &template,
            &DataValue::Object(BTreeMap::from([("rows".into(), DataValue::Array(rows))])),
            &[],
        )
        .unwrap()
}
fn plan() -> AdaptiveLayoutPlan {
    AdaptiveLayoutPlan::parse_yaml(
        include_bytes!("../wiki/audit/adaptive-plan.yml"),
        &ResourceLimits::default(),
    )
    .unwrap()
}
#[test]
fn final_reservation_is_not_charged_on_every_page() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let scene = engine.resolve_adaptive(&document(8), &plan()).unwrap();
    assert_eq!(scene.pages.len(), 2);
    let closing = scene.pages[1]
        .elements
        .iter()
        .find(|e| e.id.as_str() == "closing")
        .unwrap();
    assert_eq!(closing.bounds.layout.origin.y, Unit::points(180).unwrap());
}

fn resolve(document: &DocumentIr, yaml: &str) -> Result<ResolvedScene> {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let plan = AdaptiveLayoutPlan::parse_yaml(yaml.as_bytes(), &limits)?;
    LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?.resolve_adaptive(document, &plan)
}
const PLAN: &str = include_str!("../wiki/audit/adaptive-plan.yml");

#[test]
fn one_two_five_twenty_pages_preserve_every_row_once_and_final_closing() {
    for (pages, rows) in [(1, 3), (2, 8), (5, 26), (20, 116)] {
        let doc = document(rows);
        let scene = resolve(&doc, PLAN).unwrap();
        assert_eq!(scene.pages.len(), pages);
        let indices: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|p| &p.elements)
            .filter_map(|e| e.table.as_ref())
            .flat_map(|t| t.rows.iter().map(|r| r.source_index))
            .collect();
        assert_eq!(indices, (0..rows as u64).collect::<Vec<_>>());
        assert_eq!(
            scene
                .pages
                .iter()
                .flat_map(|p| &p.elements)
                .filter(|e| e.id.as_str() == "closing")
                .count(),
            1
        );
        assert!(scene
            .pages
            .last()
            .unwrap()
            .elements
            .iter()
            .any(|e| e.id.as_str() == "closing"));
        let safety = audit_layout(
            &scene,
            &ResourceLimits::default(),
            &LayoutSafetyOptions::default(),
            &OperationControl::default(),
        )
        .unwrap();
        safety.validation.enforce(true).unwrap();
        assert_eq!(scene, resolve(&doc, PLAN).unwrap());
        assert!(doc.elements[0]
            .table
            .as_ref()
            .unwrap()
            .page_bodies
            .is_none());
    }
}

#[test]
fn final_group_moves_intact_when_it_fits_destination_with_closing() {
    let mut doc = document(7);
    for row in doc.elements[0]
        .table
        .as_mut()
        .unwrap()
        .rows
        .iter_mut()
        .skip(4)
    {
        row.insert("group".into(), DataValue::Integer(99));
    }
    let scene = resolve(&doc, PLAN).unwrap();
    let last = scene.pages[1]
        .elements
        .iter()
        .find_map(|e| e.table.as_ref())
        .unwrap();
    assert_eq!(
        last.rows.iter().map(|r| r.source_index).collect::<Vec<_>>(),
        vec![4, 5, 6]
    );
}

#[test]
fn oversize_policy_is_explicit_and_indivisible_rows_fail() {
    let mut doc = document(8);
    for row in &mut doc.elements[0].table.as_mut().unwrap().rows {
        row.insert("group".into(), DataValue::Integer(1));
    }
    assert!(resolve(&doc, &PLAN.replace("split_rows", "error"))
        .unwrap_err()
        .to_string()
        .contains("oversized_group"));
    assert_eq!(resolve(&doc, PLAN).unwrap().pages.len(), 2);
    doc.elements[0].table.as_mut().unwrap().row_height = Some("180pt".parse().unwrap());
    assert!(resolve(&doc, PLAN).is_err());
}

#[test]
fn absent_closing_reserves_nothing_and_empty_dataset_has_no_blank_page() {
    let mut doc = document(6);
    doc.elements[1].hidden = true;
    let scene = resolve(&doc, PLAN).unwrap();
    assert_eq!(scene.pages.len(), 2);
    let last = scene.pages[1]
        .elements
        .iter()
        .find(|e| e.table.is_some())
        .unwrap();
    assert_eq!(last.bounds.layout.size.height, Unit::points(200).unwrap());
    assert!(!scene
        .pages
        .iter()
        .flat_map(|p| &p.elements)
        .any(|e| e.id.as_str() == "closing"));
    assert_eq!(resolve(&document(0), PLAN).unwrap().pages.len(), 1);
}

#[test]
fn growing_closing_recalculates_destination_without_repeated_reservation() {
    let mut doc = document(3);
    doc.elements[1].geometry.height = Some(Length::Auto);
    doc.elements[1].text = Some("One\nTwo\nThree\nFour\nFive\nSix\nSeven".into());
    let scene = resolve(&doc, PLAN).unwrap();
    assert_eq!(scene.pages.len(), 2);
    let closing = scene.pages[1]
        .elements
        .iter()
        .find(|e| e.id.as_str() == "closing")
        .unwrap();
    assert_eq!(
        closing.bounds.layout.size.height,
        Unit::from_raw(75_600_000)
    );
    assert_eq!(
        closing.bounds.layout.bottom().unwrap(),
        Unit::points(220).unwrap()
    );
}

#[test]
fn final_and_single_offsets_are_applied_to_cells_and_layout_bounds() {
    let altered = PLAN
        .replace(
            "last: {offset_y: 0pt, height: 200pt}",
            "last: {offset_y: 15pt, height: 185pt}",
        )
        .replace(
            "single: {offset_y: 0pt, height: 180pt}",
            "single: {offset_y: 5pt, height: 175pt}",
        );
    for (rows, y) in [(2, 25), (8, 35)] {
        let scene = resolve(&document(rows), &altered).unwrap();
        let table = scene
            .pages
            .last()
            .unwrap()
            .elements
            .iter()
            .find(|e| e.table.is_some())
            .unwrap();
        assert_eq!(table.bounds.layout.origin.y, Unit::points(y).unwrap());
        assert_eq!(
            table.table.as_ref().unwrap().header[0].bounds.origin.y,
            Unit::points(y).unwrap()
        );
    }
}

#[test]
fn old_reservations_or_totals_are_not_silently_combined() {
    let mut doc = document(2);
    doc.elements[0].table.as_mut().unwrap().page_bodies = Some(TablePageBodies {
        first: TablePageBody {
            offset_y: "0pt".parse().unwrap(),
            height: "100pt".parse().unwrap(),
        },
        continuation: TablePageBody {
            offset_y: "0pt".parse().unwrap(),
            height: "100pt".parse().unwrap(),
        },
    });
    assert!(resolve(&doc, PLAN)
        .unwrap_err()
        .to_string()
        .contains("one reservation owner"));
    doc.elements[0].table.as_mut().unwrap().page_bodies = None;
    doc.elements[0]
        .table
        .as_mut()
        .unwrap()
        .spec
        .total_fields
        .push("content".into());
    assert!(resolve(&doc, PLAN).is_err());
}

#[test]
fn strict_plan_versions_fields_and_resource_limits_are_enforced() {
    let limits = ResourceLimits::default();
    for yaml in [
        PLAN.replace("'1.0'", "'2.0'"),
        format!("{PLAN}unknown: true\n"),
        format!("{PLAN}table: entries\n"),
        PLAN.replace("height: 200pt", "height: auto"),
    ] {
        assert!(resolve(&document(2), &yaml).is_err());
    }
    let tiny = ResourceLimits {
        max_template_bytes: 8,
        ..limits.clone()
    };
    assert_eq!(
        AdaptiveLayoutPlan::parse_yaml(PLAN.as_bytes(), &tiny)
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
    let fonts = fonts();
    let bounded = ResourceLimits {
        max_pages: 1,
        ..limits.clone()
    };
    assert_eq!(
        LayoutEngine::new(&bounded, &fonts, LayoutOptions::default())
            .unwrap()
            .resolve_adaptive(&document(8), &plan())
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        LayoutEngine::new_controlled(
            &limits,
            &fonts,
            LayoutOptions::default(),
            OperationControl::new(token)
        )
        .unwrap()
        .resolve_adaptive(&document(2), &plan())
        .unwrap_err()
        .code(),
        ErrorCode::Cancelled
    );
}

#[test]
fn invalid_reserve_anchors_and_outside_regions_fail_with_no_scene() {
    let doc = document(2);
    for yaml in [
        PLAN.replace("gap: 10pt", "gap: 12pt"),
        PLAN.replace("gap: 10pt", "gap: -1pt"),
        PLAN.replace("height: 200pt", "height: 400pt"),
        PLAN.replace("height: 180pt", "height: 30pt"),
    ] {
        assert!(resolve(&doc, &yaml).is_err());
    }
    let mut outside = doc.clone();
    outside.elements[0].geometry.x = Some("-10pt".parse().unwrap());
    assert!(resolve(&outside, PLAN).is_err());
}

#[test]
fn impossible_first_group_does_not_return_a_blank_leading_page() {
    let mut doc = document(3);
    for row in &mut doc.elements[0].table.as_mut().unwrap().rows {
        row.insert("group".into(), DataValue::Integer(1));
    }
    doc.elements[1].geometry.height = Some(Length::Auto);
    doc.elements[1].text = Some("One\nTwo\nThree\nFour\nFive\nSix\nSeven".into());
    let error = resolve(&doc, PLAN).unwrap_err();
    assert!(error.to_string().contains("empty first page"));
}

#[test]
fn empty_table_header_must_fit_after_reserving_closing() {
    let doc = document(0);
    let plan = PLAN.replace(
        "single: {offset_y: 0pt, height: 180pt}",
        "single: {offset_y: 0pt, height: 55pt}",
    );
    assert!(resolve(&doc, &plan)
        .unwrap_err()
        .to_string()
        .contains("header exceeds"));
}

struct CancelOnFourthRow(CancellationToken);
impl ProgressObserver for CancelOnFourthRow {
    fn report(&self, event: &ProgressEvent) {
        if event.phase == ProgressPhase::Layout && event.completed == 3 {
            self.0.cancel();
        }
    }
}
#[test]
fn cancellation_is_observed_during_row_visitation() {
    let token = CancellationToken::default();
    let control = OperationControl::new(token.clone())
        .with_observer(std::sync::Arc::new(CancelOnFourthRow(token.clone())));
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine =
        LayoutEngine::new_controlled(&limits, &fonts, LayoutOptions::default(), control).unwrap();
    assert_eq!(
        engine
            .resolve_adaptive(&document(116), &plan())
            .unwrap_err()
            .code(),
        ErrorCode::Cancelled
    );
    assert!(token.is_cancelled());
}

#[test]
fn first_body_follows_measured_header_and_preserves_its_bottom() {
    for (lines, expected) in [(2, 49_600_000), (4, 71_200_000)] {
        let mut doc = document(1);
        let mut title = doc.elements[1].clone();
        title.id = ElementId::new("title").unwrap();
        title.geometry.anchors.clear();
        title.geometry.height = Some(Length::Auto);
        title.text = Some(vec!["Heading"; lines].join("\n"));
        doc.elements[0].collision = Some(CollisionPolicy {
            ignore: std::collections::BTreeSet::from(["title".into()]),
            ..CollisionPolicy::default()
        });
        doc.elements.insert(0, title);
        let yaml = format!("{PLAN}first_after: {{element: title, gap: 8pt}}\n");
        let scene = resolve(&doc, &yaml).unwrap();
        assert_eq!(scene.pages.len(), 1);
        let table = scene.pages[0]
            .elements
            .iter()
            .find(|e| e.table.is_some())
            .unwrap();
        assert_eq!(table.bounds.layout.origin.y, Unit::from_raw(expected));
        assert_eq!(
            table.table.as_ref().unwrap().header[0].bounds.origin.y,
            Unit::from_raw(expected)
        );
        assert_eq!(
            table.bounds.layout.bottom().unwrap(),
            Unit::points(150).unwrap()
        );
    }
}

#[test]
fn pagination_diagnostics_are_bounded_and_do_not_change_layout() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let document = document(8);
    let expected = engine.resolve_adaptive(&document, &plan()).unwrap();
    let report = engine.diagnose_adaptive(&document, &plan(), 100);
    assert_eq!(report.outcome().as_ref().unwrap(), &expected);
    assert!(report
        .decisions()
        .iter()
        .any(|d| d.rule == PaginationRule::MoveGroup));
    for decision in report.decisions() {
        assert_eq!(
            decision.body_height,
            decision
                .capacity
                .checked_add(decision.header_height)
                .unwrap()
        );
        assert_eq!(
            decision.available_height,
            decision.capacity.checked_sub(decision.used_height).unwrap()
        );
        assert_eq!(
            decision.required_height,
            decision
                .group_height
                .checked_add(decision.reserved_height)
                .unwrap()
        );
    }
    let json = serde_json::to_string(report.decisions()).unwrap();
    assert!(!json.contains("Entry"));
    assert!(!json.contains("Comment"));
    let limited = engine.diagnose_adaptive(&document, &plan(), 1);
    assert_eq!(
        limited.outcome().as_ref().unwrap_err().code(),
        ErrorCode::LimitExceeded
    );
    assert_eq!(limited.decisions().len(), 1);
    assert_eq!(
        engine
            .diagnose_adaptive(&document, &plan(), 0)
            .decisions()
            .len(),
        0
    );
}

#[test]
fn diagnostic_final_reserve_and_failure_evidence_are_exact() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let doc = document(8);
    let report = engine.diagnose_adaptive(&doc, &plan(), 100);
    let again = engine.diagnose_adaptive(&doc, &plan(), 100);
    assert_eq!(report.decisions(), again.decisions());
    let fits: Vec<_> = report
        .decisions()
        .iter()
        .filter(|d| d.rule == PaginationRule::Fit)
        .collect();
    assert_eq!(fits.len(), 8);
    for d in &fits[..7] {
        assert_eq!(d.reserved_height, Unit::ZERO);
    }
    let last = fits.last().unwrap();
    assert_eq!(last.page, 1);
    assert_eq!((last.first_row, last.last_row), (7, 7));
    assert_eq!(last.final_reservation, Unit::points(50).unwrap());
    assert_eq!(last.required_height, Unit::points(80).unwrap());
    assert_eq!(fits[0].capacity, Unit::points(160).unwrap());
    assert_eq!(last.capacity, Unit::points(180).unwrap());
    let mut oversized = document(8);
    for row in &mut oversized.elements[0].table.as_mut().unwrap().rows {
        row.insert(
            "group".into(),
            DataValue::String("private-group-value".into()),
        );
    }
    let error_plan =
        AdaptiveLayoutPlan::parse_yaml(PLAN.replace("split_rows", "error").as_bytes(), &limits)
            .unwrap();
    let rejected = engine.diagnose_adaptive(&oversized, &error_plan, 100);
    assert!(rejected.outcome().is_err());
    let rejection = rejected.decisions().last().unwrap();
    assert_eq!(rejection.rule, PaginationRule::Reject);
    assert_eq!((rejection.first_row, rejection.last_row), (0, 7));
    assert!(rejection.required_height > rejection.capacity);
    assert!(!serde_json::to_string(rejected.decisions())
        .unwrap()
        .contains("private-group-value"));
    let split = engine.diagnose_adaptive(&oversized, &plan(), 100);
    assert!(split.outcome().is_ok());
    assert!(split
        .decisions()
        .iter()
        .any(|d| d.rule == PaginationRule::SplitRows));
    assert!(split
        .decisions()
        .iter()
        .any(|d| d.rule == PaginationRule::MoveRow));
    let too_large = engine.diagnose_adaptive(&doc, &plan(), limits.max_elements + 1);
    assert_eq!(
        too_large.outcome().as_ref().unwrap_err().code(),
        ErrorCode::LimitExceeded
    );
}

#[test]
fn diagnostic_reservation_is_recomputed_for_destination_page() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let plan = AdaptiveLayoutPlan::parse_yaml(
        PLAN.replace(
            "single: {offset_y: 0pt, height: 180pt}",
            "single: {offset_y: 0pt, height: 160pt}",
        )
        .as_bytes(),
        &limits,
    )
    .unwrap();
    let report = engine.diagnose_adaptive(&document(4), &plan, 100);
    assert!(report.outcome().is_ok());
    let moving = report
        .decisions()
        .iter()
        .find(|d| d.rule == PaginationRule::MoveGroup)
        .unwrap();
    assert_eq!(moving.first_row, 3);
    assert_eq!(moving.page, 0);
    assert_eq!(moving.final_reservation, Unit::points(70).unwrap());
    let destination = report.decisions().last().unwrap();
    assert_eq!(destination.rule, PaginationRule::Fit);
    assert_eq!(destination.page, 1);
    assert_eq!(destination.final_reservation, Unit::points(50).unwrap());
    assert_eq!(destination.used_height, Unit::ZERO);
}

#[test]
fn diagnostic_header_reservation_follows_repeat_header_policy() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let mut doc = document(8);
    doc.elements[0].table.as_mut().unwrap().spec.repeat_header = false;
    let report = engine.diagnose_adaptive(&doc, &plan(), 100);
    assert!(report.outcome().is_ok());
    assert_eq!(
        report.decisions()[0].header_height,
        Unit::points(20).unwrap()
    );
    let last = report.decisions().last().unwrap();
    assert_eq!(last.page, 1);
    assert_eq!(last.header_height, Unit::ZERO);
    assert_eq!(last.body_height, Unit::points(200).unwrap());
    assert_eq!(last.capacity, last.body_height);
}
