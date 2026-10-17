// =============================================================================
//        #######
//     ###       ###     F: offscreen.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#![cfg(feature = "metal")]
use appcore_render::{OffscreenRenderer, RenderError, RenderLimits};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
fn run<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        assert!(Instant::now() < deadline, "GPU initialization timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn staging_budget_is_checked_before_device_request() {
    let limits = RenderLimits {
        managed_bytes: 64 * 64 * 8,
        ..RenderLimits::default()
    };
    assert!(matches!(
        run(OffscreenRenderer::new(limits, 64, 64)),
        Err(RenderError::BudgetExceeded)
    ));
}
#[test]
#[ignore = "requires a real native Metal adapter; run explicitly to qualify GPU"]
fn metal_capture_proves_clear_depth_occlusion_and_reuse() -> Result<(), RenderError> {
    // Unaligned row width also tests stripping of GPU copy padding.
    let mut renderer = run(OffscreenRenderer::new(RenderLimits::default(), 65, 64))?;
    assert_eq!(format!("{:?}", renderer.capabilities().backend), "Metal");
    assert_eq!(renderer.managed_bytes(), 65 * 64 * 8 + 512 * 64);
    for _ in 0..3 {
        let pixels = renderer.capture_triangle()?;
        assert_eq!(pixels.len(), 65 * 64 * 4);
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
        let center = (32 * 65 + 32) * 4;
        // Far blue geometry is submitted after orange: orange surviving proves depth test.
        assert_eq!(&pixels[center..center + 4], &[255, 64, 0, 255]);
    }
    Ok(())
}
