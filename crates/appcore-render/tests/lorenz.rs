// =============================================================================
//        #######
//     ###       ###     F: lorenz.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#![cfg(feature = "metal")]
use appcore_render::{gpu, LorenzCompute, LorenzParameters, RenderError, RenderLimits};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
        assert!(Instant::now() < deadline, "GPU initialization timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn derivative(p: [f64; 3], coefficients: LorenzParameters) -> [f64; 3] {
    [
        f64::from(coefficients.sigma) * (p[1] - p[0]),
        p[0] * (f64::from(coefficients.rho) - p[2]) - p[1],
        p[0] * p[1] - f64::from(coefficients.beta) * p[2],
    ]
}
fn reference(p: [f64; 3], coefficients: LorenzParameters) -> [f64; 3] {
    let dt = 0.005;
    let a = derivative(p, coefficients);
    let b = derivative(
        std::array::from_fn(|i| p[i] + a[i] * dt / 2.0),
        coefficients,
    );
    let c = derivative(
        std::array::from_fn(|i| p[i] + b[i] * dt / 2.0),
        coefficients,
    );
    let d = derivative(std::array::from_fn(|i| p[i] + c[i] * dt), coefficients);
    std::array::from_fn(|i| p[i] + dt * (a[i] + 2.0 * b[i] + 2.0 * c[i] + d[i]) / 6.0)
}
#[test]
#[ignore = "requires a real Metal device; invoke explicitly"]
fn gpu_lorenz_matches_short_f64_reference_and_reset() -> Result<(), RenderError> {
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))
        .map_err(|_| RenderError::DeviceUnavailable)?;
    let (device, queue) = run(adapter.request_device(&Default::default()))
        .map_err(|_| RenderError::DeviceUnavailable)?;
    assert!(matches!(
        LorenzCompute::new(
            &device,
            RenderLimits {
                managed_bytes: 16,
                ..Default::default()
            }
        ),
        Err(RenderError::BudgetExceeded)
    ));
    let mut simulation = LorenzCompute::new(&device, RenderLimits::default())?;
    for _ in 0..2 {
        assert!(simulation.submit_steps(&device, &queue, 8)?.is_some());
    }
    assert_eq!(simulation.count(), 16);
    assert_eq!(simulation.cursor(), 16);
    let bytes = capture_heads(&device, &queue, simulation.heads())?;
    for trail in 0..6 {
        let mut expected = [f64::from(1.0 + trail as f32 * 0.001), 1.0, 1.0];
        for _ in 0..16 {
            expected = reference(expected, LorenzParameters::default());
        }
        for (axis, expected) in expected.iter().enumerate() {
            let offset = trail * 16 + axis * 4;
            let actual = f32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .map_err(|_| RenderError::CaptureFailed)?,
            );
            assert!(
                (f64::from(actual) - expected).abs() < 0.0002,
                "trail={trail} axis={axis} actual={actual} expected={expected}"
            );
        }
    }
    assert!(matches!(
        simulation.submit_steps(&device, &queue, 9),
        Err(RenderError::InvalidLimits)
    ));
    assert_eq!(simulation.count(), 16);
    assert!(simulation.submit_steps(&device, &queue, 0)?.is_none());
    assert_eq!(simulation.count(), 16);
    assert_eq!(
        simulation.reset_with_parameters(
            &queue,
            LorenzParameters {
                rho: f32::NAN,
                ..Default::default()
            }
        ),
        Err(RenderError::InvalidLimits)
    );
    assert_eq!(simulation.count(), 16);
    assert_eq!(simulation.parameters(), LorenzParameters::default());
    assert_eq!(capture_heads(&device, &queue, simulation.heads())?, bytes);
    simulation.reset(&queue);
    assert_eq!(simulation.count(), 0);
    assert_eq!(simulation.cursor(), 0);
    let reset = capture_heads(&device, &queue, simulation.heads())?;
    for trail in 0..6 {
        let expected = [1.0 + trail as f32 * 0.001, 1.0, 1.0, 1.0];
        for (axis, expected) in expected.iter().enumerate() {
            let offset = trail * 16 + axis * 4;
            let actual = f32::from_le_bytes(
                reset[offset..offset + 4]
                    .try_into()
                    .map_err(|_| RenderError::CaptureFailed)?,
            );
            assert_eq!(actual, *expected);
        }
    }
    custom_parameters(&device, &queue, &mut simulation)?;
    Ok(())
}
fn capture_heads(
    device: &gpu::Device,
    queue: &gpu::Queue,
    heads: &gpu::Buffer,
) -> Result<Vec<u8>, RenderError> {
    let buffer = device.create_buffer(&gpu::BufferDescriptor {
        label: None,
        size: 96,
        usage: gpu::BufferUsages::COPY_DST | gpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(heads, 0, &buffer, 0, 96);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(gpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        device
            .poll(gpu::PollType::Poll)
            .map_err(|_| RenderError::DeviceUnavailable)?;
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(_)) => return Err(RenderError::CaptureFailed),
            Err(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(RenderError::CaptureTimeout);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mapped = buffer.slice(..).get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(bytes)
}

fn custom_parameters(
    device: &gpu::Device,
    queue: &gpu::Queue,
    simulation: &mut LorenzCompute,
) -> Result<(), RenderError> {
    let parameters = LorenzParameters {
        sigma: 12.0,
        rho: 35.0,
        beta: 3.0,
    };
    simulation.reset_with_parameters(queue, parameters)?;
    assert_eq!(simulation.parameters(), parameters);
    assert_eq!(simulation.count(), 0);
    simulation.submit_steps(device, queue, 8)?;
    let bytes = capture_heads(device, queue, simulation.heads())?;
    for trail in 0..6 {
        let mut expected = [f64::from(1.0 + trail as f32 * 0.001), 1.0, 1.0];
        for _ in 0..8 {
            expected = reference(expected, parameters);
        }
        for (axis, expected) in expected.into_iter().enumerate() {
            let offset = trail * 16 + axis * 4;
            let actual = f32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .map_err(|_| RenderError::CaptureFailed)?,
            );
            assert!((f64::from(actual) - expected).abs() < 0.0002);
        }
    }
    simulation.reset(queue);
    assert_eq!(simulation.parameters(), parameters);
    Ok(())
}
