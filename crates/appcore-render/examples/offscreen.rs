// =============================================================================
//        #######
//     ###       ###     F: offscreen.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#[cfg(feature = "metal")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_render::{OffscreenRenderer, RenderLimits};
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(OffscreenRenderer::new(RenderLimits::default(), 640, 480));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut renderer = loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            break result?;
        }
        if Instant::now() >= deadline {
            return Err("GPU initialization timeout".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let rgba = renderer.capture_triangle()?;
    let mut ppm = b"P6\n640 480\n255\n".to_vec();
    for pixel in rgba.chunks_exact(4) {
        ppm.extend_from_slice(&pixel[..3]);
    }
    let output = std::env::temp_dir().join("e3dn-offscreen.ppm");
    std::fs::write(&output, ppm)?;
    println!(
        "backend={:?} managed_bytes={} image={}",
        renderer.capabilities().backend,
        renderer.managed_bytes(),
        output.display()
    );
    Ok(())
}
#[cfg(not(feature = "metal"))]
fn main() {
    eprintln!("Select --features metal on macOS");
}
