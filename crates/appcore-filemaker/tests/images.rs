// =============================================================================
//        #######
//     ###       ###     F: images.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use appcore_filemaker::*;

    fn bounds() -> Rect {
        Rect::new(
            Unit::ZERO,
            Unit::ZERO,
            Unit::points(100).unwrap(),
            Unit::points(100).unwrap(),
        )
        .unwrap()
    }

    fn svg(name: &str, width: u32, height: u32) -> Asset {
        Asset::new(
            name,
            "image/svg+xml",
            format!(r#"<svg viewBox="0 0 {width} {height}"></svg>"#).into_bytes(),
        )
    }

    #[test]
    fn every_fit_mode_resolves_before_export() {
        let asset = svg("large.svg", 200, 100);
        let contain =
            resolve_image_placement(&asset, bounds(), ImageOptions::default(), 1_000_000).unwrap();
        assert!(contain.vector);
        assert_eq!(contain.destination.size.width, Unit::points(100).unwrap());
        assert_eq!(contain.destination.size.height, Unit::points(50).unwrap());
        assert_eq!(contain.destination.origin.y, Unit::points(25).unwrap());

        let fill = resolve_image_placement(
            &asset,
            bounds(),
            ImageOptions {
                fit: ImageFit::Fill,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(fill.destination, bounds());

        let none = resolve_image_placement(
            &asset,
            bounds(),
            ImageOptions {
                fit: ImageFit::None,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(none.destination.size.width, Unit::points(150).unwrap());
        assert_eq!(none.destination.size.height, Unit::points(75).unwrap());
        assert_eq!(none.destination.origin.x, Unit::points(-25).unwrap());
        assert_eq!(
            none.destination.origin.y,
            Unit::from_raw(Unit::PER_POINT * 25 / 2)
        );

        let scale_down = resolve_image_placement(
            &asset,
            bounds(),
            ImageOptions {
                fit: ImageFit::ScaleDown,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(scale_down.destination, contain.destination);

        let small = resolve_image_placement(
            &svg("small.svg", 40, 20),
            bounds(),
            ImageOptions {
                fit: ImageFit::ScaleDown,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(small.destination.size.width, Unit::points(30).unwrap());
        assert_eq!(small.destination.size.height, Unit::points(15).unwrap());
    }

    #[test]
    fn crop_aspect_focal_and_pixel_bounds_are_explicit() {
        let placement = resolve_image_placement(
            &svg("crop.svg", 200, 100),
            bounds(),
            ImageOptions {
                fit: ImageFit::Cover,
                focal_x: 1_000_000,
                crop: ImageCrop {
                    left: 100_000,
                    right: 200_000,
                    ..ImageCrop::default()
                },
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(placement.source.x, 60);
        assert_eq!(placement.source.width, 100);
        assert_eq!(placement.source.height, 100);
        assert_eq!(placement.destination, bounds());

        let invalid = resolve_image_placement(
            &svg("invalid.svg", 200, 100),
            bounds(),
            ImageOptions {
                crop: ImageCrop {
                    left: 500_000,
                    right: 500_000,
                    ..ImageCrop::default()
                },
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap_err();
        assert_eq!(invalid.code(), ErrorCode::AssetInvalid);

        let limited = resolve_image_placement(
            &svg("limited.svg", 200, 100),
            bounds(),
            ImageOptions::default(),
            19_999,
        )
        .unwrap_err();
        assert_eq!(limited.code(), ErrorCode::LimitExceeded);

        let empty_destination = resolve_image_placement(
            &svg("empty.svg", 200, 100),
            Rect::new(Unit::ZERO, Unit::ZERO, Unit::ZERO, Unit::ZERO).unwrap(),
            ImageOptions {
                fit: ImageFit::Cover,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap_err();
        assert_eq!(empty_destination.code(), ErrorCode::AssetInvalid);
    }

    #[test]
    fn raster_exif_orientation_is_optional_and_swaps_intrinsic_axes() {
        let asset = Asset::new("oriented.jpg", "image/jpeg", oriented_jpeg());
        let respected =
            resolve_image_placement(&asset, bounds(), ImageOptions::default(), 1_000_000).unwrap();
        assert!(!respected.vector);
        assert_eq!(respected.orientation, ImageOrientation::Rotate90);
        assert_eq!(
            (respected.intrinsic_width, respected.intrinsic_height),
            (1, 2)
        );

        let ignored = resolve_image_placement(
            &asset,
            bounds(),
            ImageOptions {
                respect_exif: false,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(ignored.orientation, ImageOrientation::Identity);
        assert_eq!((ignored.intrinsic_width, ignored.intrinsic_height), (2, 1));
    }

    fn oriented_jpeg() -> Vec<u8> {
        let pixels = ::image::RgbImage::from_pixel(2, 1, ::image::Rgb([20, 40, 60]));
        let mut output = Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut output, ::image::ImageFormat::Jpeg)
            .unwrap();
        let mut jpeg = output.into_inner();
        let payload = [
            b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1,
            0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let length = u16::try_from(payload.len() + 2).unwrap().to_be_bytes();
        let mut segment = vec![0xff, 0xe1, length[0], length[1]];
        segment.extend_from_slice(&payload);
        jpeg.splice(2..2, segment);
        jpeg
    }
}
