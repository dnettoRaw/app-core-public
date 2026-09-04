// =============================================================================
//        #######
//     ###       ###     F: properties.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use appcore_filemaker::{Point, Rect, Transform, Unit};

#[test]
fn rectangle_intersection_is_symmetric_across_generated_cases() {
    let mut state = 0x5eed_u64;
    for _ in 0..10_000 {
        let values = std::array::from_fn::<_, 8, _>(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            i64::try_from((state >> 32) % 100).unwrap()
        });
        let left = Rect::new(
            Unit::points(values[0]).unwrap(),
            Unit::points(values[1]).unwrap(),
            Unit::points(values[2]).unwrap(),
            Unit::points(values[3]).unwrap(),
        )
        .unwrap();
        let right = Rect::new(
            Unit::points(values[4]).unwrap(),
            Unit::points(values[5]).unwrap(),
            Unit::points(values[6]).unwrap(),
            Unit::points(values[7]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            left.intersects(right).unwrap(),
            right.intersects(left).unwrap()
        );
        assert_eq!(
            left.intersection(right).unwrap(),
            right.intersection(left).unwrap()
        );
        let union = left.union(right).unwrap();
        assert!(union.origin.x <= left.origin.x && union.origin.y <= left.origin.y);
        assert!(union.origin.x <= right.origin.x && union.origin.y <= right.origin.y);
        assert!(union.right().unwrap() >= left.right().unwrap());
        assert!(union.right().unwrap() >= right.right().unwrap());
        assert!(union.bottom().unwrap() >= left.bottom().unwrap());
        assert!(union.bottom().unwrap() >= right.bottom().unwrap());
    }
}

#[test]
fn identity_transform_preserves_generated_points() {
    for raw in (-1_000_000_i64..=1_000_000).step_by(7_919) {
        let point = Point {
            x: Unit::from_raw(raw),
            y: Unit::from_raw(raw.saturating_mul(-3)),
        };
        assert_eq!(Transform::IDENTITY.apply(point).unwrap(), point);
    }
}

#[test]
fn touching_edges_do_not_collide() {
    let first = Rect::new(
        Unit::ZERO,
        Unit::ZERO,
        Unit::points(10).unwrap(),
        Unit::points(10).unwrap(),
    )
    .unwrap();
    let second = Rect::new(
        Unit::points(10).unwrap(),
        Unit::ZERO,
        Unit::points(2).unwrap(),
        Unit::points(2).unwrap(),
    )
    .unwrap();
    assert!(!first.intersects(second).unwrap());
}

#[test]
fn transform_bounds_include_translation() {
    let transform = Transform {
        tx: Unit::points(2).unwrap(),
        ty: Unit::points(3).unwrap(),
        ..Transform::IDENTITY
    };
    let bounds = transform
        .bounds(
            Rect::new(
                Unit::ZERO,
                Unit::ZERO,
                Unit::points(4).unwrap(),
                Unit::points(5).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        bounds.origin,
        Point {
            x: Unit::points(2).unwrap(),
            y: Unit::points(3).unwrap()
        }
    );
}

#[test]
fn rotation_composition_and_inverse_are_fixed_point() {
    let rotation = Transform::rotation_degrees(90).unwrap();
    let translated = rotation
        .then(Transform::translation(
            Unit::points(5).unwrap(),
            Unit::points(7).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        translated
            .apply(Point {
                x: Unit::points(2).unwrap(),
                y: Unit::points(3).unwrap(),
            })
            .unwrap(),
        Point {
            x: Unit::points(2).unwrap(),
            y: Unit::points(9).unwrap(),
        }
    );
    let diagonal = Transform::rotation_degrees(45).unwrap();
    assert!((diagonal.a - 707_107).abs() <= 2);
    assert!((diagonal.b - 707_107).abs() <= 2);
    let page_vector = rotation
        .apply(Point {
            x: Unit::points(4).unwrap(),
            y: Unit::points(2).unwrap(),
        })
        .unwrap();
    assert_eq!(
        rotation.inverse_vector(page_vector).unwrap(),
        Point {
            x: Unit::points(4).unwrap(),
            y: Unit::points(2).unwrap(),
        }
    );
}
